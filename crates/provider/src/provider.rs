//! Seismic provider for HTTP requests
use alloy_network::TransactionBuilder;
use alloy_primitives::Bytes;
use alloy_provider::{
    fillers::{ChainIdFiller, FillProvider, JoinFill, NonceFiller, TxFiller, WalletFiller},
    PendingTransactionBuilder, Provider, ProviderBuilder, ProviderLayer, RootProvider, SendableTx,
    WsConnect,
};
use alloy_rpc_client::RpcClient;
use alloy_transport::TransportResult;
use seismic_alloy_consensus::InputDecryptionElements;
use seismic_alloy_network::{
    fillers::{SeismicElementsFiller, SeismicGasFiller},
    foundry::SeismicFoundry,
    seismic_network::SeismicNetwork,
    wallet::SeismicWallet,
    SeismicReth,
};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use std::ops::Deref;

use crate::SeismicProviderExt;

/// Seismic middleware for signed providers (with response decryption support)
/// Implements [`SeismicProviderExt`] trait with full encryption/decryption
#[derive(Debug, Clone)]
pub struct SeismicSignedProviderLayer<N, P> {
    /// Inner provider.
    inner: P,
    /// Ephemeral secret key for request encryption and response decryption
    ephemeral_secret_key: seismic_enclave::secp256k1::SecretKey,
    /// TEE public key for response decryption
    tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    _network: std::marker::PhantomData<N>,
}

impl<N: SeismicNetwork, P> SeismicSignedProviderLayer<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
{
    /// Create a new signed seismic provider with ephemeral secret key and TEE pubkey
    pub(crate) fn new_signed(
        inner: P,
        secret_key: seismic_enclave::secp256k1::SecretKey,
        tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    ) -> Self {
        Self {
            inner,
            ephemeral_secret_key: secret_key,
            tee_pubkey,
            _network: std::marker::PhantomData,
        }
    }
}

/// Seismic middleware for unsigned providers (encryption only, no response decryption)
/// Implements [`SeismicProviderExt`] trait with encryption support only
#[derive(Debug, Clone)]
pub struct SeismicUnsignedProviderLayer<N, P> {
    /// Inner provider.
    inner: P,
    _network: std::marker::PhantomData<N>,
}

impl<N: SeismicNetwork, P> SeismicUnsignedProviderLayer<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
{
    /// Create a new unsigned seismic provider with only ephemeral secret key
    pub(crate) fn new_unsigned(inner: P) -> Self {
        Self { inner, _network: std::marker::PhantomData }
    }
}

/// Implement the Provider trait for SeismicSignedProviderLayer
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N: SeismicNetwork, P> Provider<N> for SeismicSignedProviderLayer<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    fn root(&self) -> &RootProvider<N> {
        self.inner.root()
    }

    async fn send_transaction_internal(
        &self,
        tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        self.inner.send_transaction_internal(tx).await
    }
}

/// Implement the Provider trait for SeismicUnsignedProviderLayer
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N: SeismicNetwork, P> Provider<N> for SeismicUnsignedProviderLayer<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    fn root(&self) -> &RootProvider<N> {
        self.inner.root()
    }

    async fn send_transaction_internal(
        &self,
        tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        self.inner.send_transaction_internal(tx).await
    }
}

/// SeismicProviderExt implementation for SIGNED providers (with response decryption)
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N, F, P> SeismicProviderExt<N> for SeismicSignedProviderLayer<N, FillProvider<F, P, N>>
where
    N: SeismicNetwork,
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    F: TxFiller<N>,
    P: Provider<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Makes seismic calls and decrypts responses for seismic transactions
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        // Check if this is a seismic transaction and extract metadata if so
        match tx {
            SendableTx::Builder(builder) => {
                // Check if this is a seismic transaction
                let seismic_tx: &SeismicTransactionRequest = builder.as_ref();
                if !seismic_tx.is_seismic() {
                    // Not seismic, just pass through
                    return self.inner.seismic_call(SendableTx::Builder(builder)).await;
                }

                // For seismic calls, mark as signed_read before filling
                // Extract SeismicTransactionRequest, set signed_read, recreate builder
                let seismic_req: &SeismicTransactionRequest = builder.as_ref();
                let modified_req = seismic_req.clone().with_signed_read();
                let modified_builder: N::TransactionRequest = modified_req.into();

                // Fill the transaction (this adds from/nonce/chain_id/elements, encrypts, and may
                // sign)
                let filled_tx = self.inner.fill(modified_builder).await?;

                // Extract metadata from the FILLED transaction for decryption
                let metadata = match &filled_tx {
                    SendableTx::Builder(filled_builder) => {
                        let sender = filled_builder.from().ok_or_else(|| {
                            alloy_transport::TransportErrorKind::custom_str(
                                "Sender address required for seismic call decryption",
                            )
                        })?;

                        filled_builder.metadata(sender).map_err(|e| {
                            alloy_transport::TransportErrorKind::custom_str(&format!(
                                "Error creating metadata: {:?}",
                                e
                            ))
                        })?
                    }
                    SendableTx::Envelope(envelope) => {
                        // WalletFiller signed the transaction - extract metadata from envelope
                        N::extract_seismic_metadata(envelope).ok_or_else(|| {
                            alloy_transport::TransportErrorKind::custom_str(
                                "Expected seismic envelope for decryption",
                            )
                        })?
                    }
                };

                // Make the RPC call with the filled transaction
                let output = self.inner.root().seismic_call(filled_tx).await?;

                // Decrypt the response using client_decrypt (TEE pubkey + client secret key)
                let decrypted = metadata
                    .seismic_elements
                    .client_decrypt(
                        &output,
                        &self.tee_pubkey,
                        &self.ephemeral_secret_key,
                        &metadata,
                    )
                    .map_err(|e| {
                        alloy_transport::TransportErrorKind::custom_str(&format!(
                            "Error decrypting response: {:?}",
                            e
                        ))
                    })?;

                Ok(decrypted)
            }
            SendableTx::Envelope(envelope) => {
                // Envelope passed directly - try to extract seismic metadata
                if let Some(metadata) = N::extract_seismic_metadata(&envelope) {
                    // It's a seismic envelope - decrypt the response
                    let output =
                        self.inner.root().seismic_call(SendableTx::Envelope(envelope)).await?;

                    let decrypted = metadata
                        .seismic_elements
                        .client_decrypt(
                            &output,
                            &self.tee_pubkey,
                            &self.ephemeral_secret_key,
                            &metadata,
                        )
                        .map_err(|e| {
                            alloy_transport::TransportErrorKind::custom_str(&format!(
                                "Error decrypting response: {:?}",
                                e
                            ))
                        })?;

                    Ok(decrypted)
                } else {
                    // Not a seismic envelope, pass through
                    self.inner.seismic_call(SendableTx::Envelope(envelope)).await
                }
            }
        }
    }
}

/// SeismicProviderExt implementation for UNSIGNED providers (no response decryption)
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N, F, P> SeismicProviderExt<N> for SeismicUnsignedProviderLayer<N, FillProvider<F, P, N>>
where
    N: SeismicNetwork,
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    F: TxFiller<N>,
    P: Provider<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Unsigned providers don't support response decryption - just pass through
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        self.inner.seismic_call(tx).await
    }
}

/// Seismic layer for SIGNED providers (with response decryption)
#[derive(Debug, Clone)]
pub(crate) struct SeismicSignedLayer {
    /// Ephemeral secret key for response decryption
    ephemeral_secret_key: seismic_enclave::secp256k1::SecretKey,
    /// TEE public key for response decryption
    tee_pubkey: seismic_enclave::secp256k1::PublicKey,
}

impl SeismicSignedLayer {
    pub(crate) fn new_signed(
        secret_key: seismic_enclave::secp256k1::SecretKey,
        tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    ) -> Self {
        Self { ephemeral_secret_key: secret_key, tee_pubkey }
    }
}

impl<N: SeismicNetwork, P> ProviderLayer<P, N> for SeismicSignedLayer
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    type Provider = SeismicSignedProviderLayer<N, P>;

    fn layer(&self, inner: P) -> Self::Provider {
        SeismicSignedProviderLayer::new_signed(
            inner,
            self.ephemeral_secret_key.clone(),
            self.tee_pubkey,
        )
    }
}

/// Seismic layer for UNSIGNED providers (no response decryption)
#[derive(Debug, Clone)]
pub(crate) struct SeismicUnsignedLayer;

impl SeismicUnsignedLayer {
    pub(crate) fn new_unsigned() -> Self {
        Self {}
    }
}

impl<N: SeismicNetwork, P> ProviderLayer<P, N> for SeismicUnsignedLayer
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    type Provider = SeismicUnsignedProviderLayer<N, P>;

    fn layer(&self, inner: P) -> Self::Provider {
        SeismicUnsignedProviderLayer::new_unsigned(inner)
    }
}

/// Seismic provider type alias for signed provider (with response decryption)
/// Filler chain: Wallet (sets from) -> (Nonce+ChainId) -> SeismicElements (generates elements &
/// encrypts) -> Gas NOTE: Wallet+Nonce+ChainId run first, then SeismicElements can create metadata
/// and encrypt, then GasFiller estimates gas
pub type SeismicSignedProviderInner<N> = SeismicSignedProviderLayer<
    N,
    FillProvider<
        JoinFill<
            JoinFill<
                JoinFill<
                    WalletFiller<SeismicWallet<N>>,
                    JoinFill<
                        alloy_provider::fillers::NonceFiller,
                        alloy_provider::fillers::ChainIdFiller,
                    >,
                >,
                SeismicElementsFiller,
            >,
            seismic_alloy_network::fillers::SeismicGasFiller,
        >,
        RootProvider<N>,
        N,
    >,
>;

/// Seismic signed provider
#[derive(Debug, Clone)]
pub struct SeismicSignedProvider<N: SeismicNetwork>(SeismicSignedProviderInner<N>)
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync;

impl<N: SeismicNetwork> SeismicSignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Creates a new seismic signed provider and fetches TEE pubkey once
    /// Block info is fetched per-transaction, but TEE pubkey is cached
    pub async fn new(
        wallet: impl Into<SeismicWallet<N>>,
        url: reqwest::Url,
    ) -> TransportResult<Self> {
        // Fetch TEE pubkey once using a basic provider
        let temp_provider = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .connect_client(RpcClient::new_http(url.clone()));
        let tee_pubkey = temp_provider.get_tee_pubkey().await?;

        Ok(Self::new_with_tee_pubkey(wallet, url, tee_pubkey))
    }

    /// Creates a new seismic signed provider with a pre-fetched TEE pubkey
    /// This allows synchronous construction when you already have the pubkey
    pub fn new_with_tee_pubkey(
        wallet: impl Into<SeismicWallet<N>>,
        url: reqwest::Url,
        tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    ) -> Self {
        // Build filler pipeline: wallet -> nonce+chain -> seismic filler -> gas filler
        // NOTE: WalletFiller sets from, Nonce+ChainId set legacy fields, then SeismicElementsFiller
        // can create metadata and encrypt, then GasFiller can estimate gas.
        // Note: signed_read is false by default; it will be set to true specifically for calls
        let seismic_filler = SeismicElementsFiller::with_tee_pubkey_and_url(tee_pubkey);

        // Extract the ephemeral secret key for response decryption
        let ephemeral_secret_key = seismic_filler.ephemeral_secret_key().clone();

        let tx_filler_layer = JoinFill::new(
            JoinFill::new(
                JoinFill::new(
                    WalletFiller::new(wallet.into()),
                    JoinFill::new(NonceFiller::default(), ChainIdFiller::default()),
                ),
                seismic_filler,
            ),
            SeismicGasFiller::with_url(url.clone()),
        );

        // Build and return the provider
        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicSignedLayer::new_signed(ephemeral_secret_key, tee_pubkey))
            .layer(tx_filler_layer)
            .connect_client(RpcClient::new_http(url));

        Self(inner)
    }
}

/// Seismic unsigned provider type alias (no response decryption)
/// Filler chain: SeismicElements (generates elements & encrypts) -> (Nonce+ChainId) -> Gas
/// NOTE: GasFiller runs LAST (after encryption) because gas estimation needs encrypted input
pub type SeismicUnsignedProviderInner<N> = SeismicUnsignedProviderLayer<
    N,
    FillProvider<
        JoinFill<
            JoinFill<
                SeismicElementsFiller,
                JoinFill<
                    alloy_provider::fillers::NonceFiller,
                    alloy_provider::fillers::ChainIdFiller,
                >,
            >,
            seismic_alloy_network::fillers::SeismicGasFiller,
        >,
        RootProvider<N>,
        N,
    >,
>;

/// Seismic unsigned provider
#[derive(Debug, Clone)]
pub struct SeismicUnsignedProvider<N: SeismicNetwork>(SeismicUnsignedProviderInner<N>)
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync;

impl<N: SeismicNetwork> SeismicUnsignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Creates a new Seismic unsigned provider with an HTTP connection
    /// Generates one ephemeral keypair at provider creation for all transactions
    /// Note: Unsigned providers don't need TEE pubkey since they don't decrypt responses
    pub fn new_http(url: reqwest::Url) -> Self {
        // Unsigned providers don't cache TEE pubkey - the filler fetches it as needed for
        // encryption
        let seismic_filler = SeismicElementsFiller::new();

        let filler_chain = JoinFill::new(
            JoinFill::new(
                seismic_filler,
                JoinFill::new(NonceFiller::default(), ChainIdFiller::default()),
            ),
            SeismicGasFiller::with_url(url.clone()),
        );

        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicUnsignedLayer::new_unsigned())
            .layer(filler_chain)
            .connect_client(RpcClient::new_http(url));

        Self(inner)
    }

    /// Creates a new Seismic unsigned provider with a websocket connection
    /// Generates one ephemeral keypair at provider creation for all transactions
    /// Note: Unsigned providers don't need TEE pubkey since they don't decrypt responses
    pub async fn new_ws(url: reqwest::Url) -> TransportResult<Self> {
        // Unsigned providers don't cache TEE pubkey - the filler fetches it as needed for
        // encryption
        let seismic_filler = SeismicElementsFiller::new();

        let filler_chain = JoinFill::new(
            JoinFill::new(
                seismic_filler,
                JoinFill::new(NonceFiller::default(), ChainIdFiller::default()),
            ),
            SeismicGasFiller::with_url(url.clone()),
        );

        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicUnsignedLayer::new_unsigned())
            .layer(filler_chain)
            .connect_ws(WsConnect::new(url))
            .await?;

        Ok(Self(inner))
    }
}

impl<N: SeismicNetwork, P> Deref for SeismicSignedProviderLayer<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
{
    type Target = P;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<N: SeismicNetwork, P> Deref for SeismicUnsignedProviderLayer<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
{
    type Target = P;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<N: SeismicNetwork> Deref for SeismicSignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
{
    type Target = SeismicSignedProviderInner<N>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<N: SeismicNetwork> Deref for SeismicUnsignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + seismic_alloy_consensus::InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
{
    type Target = SeismicUnsignedProviderInner<N>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Create a new SeismicSignedProvider for the SeismicReth network
pub async fn sreth_signed_provider(
    wallet: impl Into<SeismicWallet<SeismicReth>>,
    url: reqwest::Url,
) -> TransportResult<SeismicSignedProvider<SeismicReth>> {
    SeismicSignedProvider::new(wallet, url).await
}

/// Create a new SeismicUnsignedProvider for the SeismicReth network
pub fn sreth_unsigned_provider(url: reqwest::Url) -> SeismicUnsignedProvider<SeismicReth> {
    SeismicUnsignedProvider::new_http(url)
}

/// Create a new SeismicSignedProvider for the SeismicFoundry network
pub async fn sfoundry_signed_provider(
    wallet: impl Into<SeismicWallet<SeismicFoundry>>,
    url: reqwest::Url,
) -> TransportResult<SeismicSignedProvider<SeismicFoundry>> {
    SeismicSignedProvider::new(wallet, url).await
}

/// Create a new SeismicUnsignedProvider for the SeismicFoundry network
pub fn sfoundry_unsigned_provider(url: reqwest::Url) -> SeismicUnsignedProvider<SeismicFoundry> {
    SeismicUnsignedProvider::new_http(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{ContractTestContext, ISeismicCounter};
    use alloy_network::{ReceiptResponse, TransactionBuilder};
    use alloy_node_bindings::{Anvil, AnvilInstance};
    use alloy_primitives::{address, hex, Address, Bytes, TxKind};
    use alloy_provider::{ext::AnvilApi, Provider, SendableTx};
    use alloy_rpc_types_eth::Filter;
    use alloy_signer_local::PrivateKeySigner;
    use alloy_sol_types::SolEvent;
    use futures_util::StreamExt;
    use seismic_alloy_consensus::SeismicReceiptEnvelope;
    use seismic_alloy_network::foundry::builder::seismic_foundry_tx_builder;

    /// Path to local sanvil binary for local testing
    const SANVIL_PATH: &str = "sanvil";

    #[tokio::test]
    async fn test_get_tee_pubkey() {
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url())
                .await
                .unwrap();

        // If this fails with a message like "Method Not Found",
        // then you may be using stock anvil instead of sanvil (seismic anvil)
        let tee_pubkey = provider.get_tee_pubkey().await.unwrap();

        assert_eq!(tee_pubkey, seismic_enclave::get_unsecure_sample_secp256k1_pk());
    }

    #[tokio::test]
    async fn test_send_transaction_with_empty_input() {
        let plaintext = Bytes::new();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url())
                .await
                .unwrap();

        let tx = seismic_foundry_tx_builder().with_input(plaintext).with_to(Address::ZERO).into();
        let res = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = res.get_receipt().await.unwrap();
        assert_eq!(receipt.inner.inner.status(), true);
    }

    /// Check that SeismicUnsignedProvider correctly inherits alloy_provider ext traits
    #[tokio::test]
    async fn test_anvil_set_code() {
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let provider = SeismicUnsignedProvider::<SeismicFoundry>::new_http(anvil.endpoint_url());

        let address = address!("0xd8da6bf26964af9d7eed9e03e53415d37aa96045");
        provider.anvil_set_code(address, Bytes::from("0xbeef")).await.unwrap();

        let code = provider.get_code_at(address).await.unwrap();
        assert_eq!(code, Bytes::from("0xbeef"));
    }

    #[tokio::test]
    async fn test_seismic_unsigned_call() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let from = get_wallet(&anvil).default_signer().address();
        let unsigned_provider =
            SeismicUnsignedProvider::<SeismicFoundry>::new_http(anvil.endpoint_url());

        // Make a regular (non-seismic) eth_call
        let tx = seismic_foundry_tx_builder()
            .with_input(plaintext)
            .with_kind(TxKind::Create)
            .with_from(from)
            .into();

        let res = unsigned_provider.seismic_call(SendableTx::Builder(tx.into())).await.unwrap();
        assert_eq!(res, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_seismic_signed_call() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url())
                .await
                .unwrap();

        // Deploy contract with a regular (non-seismic) transaction
        // Note: Create transactions should never be seismic
        let tx: SeismicTransactionRequest =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();

        // Now make a seismic call to the deployed contract using isOdd()
        let call_input = ContractTestContext::get_is_odd_input_plaintext();
        let tx = seismic_foundry_tx_builder()
            .with_input(call_input)
            .with_kind(TxKind::Call(contract_address))
            .into()
            .seismic();

        let res = provider.seismic_call(SendableTx::Builder(tx.into())).await;
        assert!(res.is_ok(), "seismic_call failed: {:?}", res.unwrap_err());
        let res = res.unwrap();

        // Verify we got the expected result from isOdd() - number is 0 (even), so isOdd should
        // return false
        let expected = Bytes::from_static(&hex!(
            "0000000000000000000000000000000000000000000000000000000000000000"
        ));
        assert_eq!(res, expected);
    }

    #[tokio::test]
    async fn test_send_transaction() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url())
                .await
                .unwrap();

        // Test sending a regular (non-seismic) Create transaction
        let tx: SeismicTransactionRequest =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();

        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_send_seismic_transaction() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url())
                .await
                .unwrap();

        // Deploy contract with a regular (non-seismic) transaction
        // Note: Create transactions should never be seismic
        let tx: SeismicTransactionRequest =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();
        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());

        // Use new .seismic() API - fillers handle encryption automatically
        let tx_input = ContractTestContext::get_set_number_input_plaintext();
        let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
            .with_input(tx_input) // Pass plaintext directly
            .with_kind(TxKind::Call(contract_address))
            .into()
            .seismic();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();

        assert!(receipt.status());
        match receipt.inner.inner {
            SeismicReceiptEnvelope::Seismic(_r) => {}
            _ => {
                panic!("expected seismic receipt");
            }
        }
    }

    #[tokio::test]
    async fn test_subscribe_to_events() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).block_time(2).spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::<SeismicFoundry>::new(wallet, anvil.endpoint_url())
            .await
            .unwrap();
        let ws_provider =
            SeismicUnsignedProvider::<SeismicFoundry>::new_ws(anvil.ws_endpoint_url())
                .await
                .unwrap();

        // Deploy contract with a regular (non-seismic) transaction
        // Note: Create transactions should never be seismic
        let tx: SeismicTransactionRequest =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();
        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();
        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());
        let filter = Filter::new().address(contract_address);

        // subscribe to events
        let event_sub = ws_provider.subscribe_logs(&filter).await.unwrap();

        // set number - use new .seismic() API
        let tx_input_set_number = ContractTestContext::get_set_number_input_plaintext();
        let tx_set_number: SeismicTransactionRequest = seismic_foundry_tx_builder()
            .with_input(tx_input_set_number) // Pass plaintext directly
            .with_kind(TxKind::Call(contract_address))
            .into()
            .seismic();

        let pending_tx_set_number = provider.send_transaction(tx_set_number.into()).await.unwrap();
        let receipt_set_number = pending_tx_set_number.get_receipt().await.unwrap();

        assert!(receipt_set_number.status());

        // increment number - use new .seismic() API
        let tx_input_increment_number = ContractTestContext::get_increment_input_plaintext();
        let tx_increment_number: SeismicTransactionRequest = seismic_foundry_tx_builder()
            .with_input(tx_input_increment_number) // Pass plaintext directly
            .with_kind(TxKind::Call(contract_address))
            .into()
            .seismic();

        let pending_tx_increment_number =
            provider.send_transaction(tx_increment_number.into()).await.unwrap();
        let receipt_increment_number = pending_tx_increment_number.get_receipt().await.unwrap();

        assert!(receipt_increment_number.status());

        // Check for events with timeout
        let mut event_stream = event_sub.into_stream();
        let mut num_set_events_received = 0;
        let mut num_increment_events_received = 0;
        let mut total_events_received = 0;

        for _ in 0..5 {
            tokio::select! {
                event_opt = event_stream.next() => {
                    match event_opt {
                        Some(log) => {
                            if log.topic0() == Some(&ISeismicCounter::setNumberEmit::SIGNATURE_HASH) {
                                num_set_events_received += 1;
                                total_events_received += 1;
                            }
                            else if log.topic0() == Some(&ISeismicCounter::incrementEmit::SIGNATURE_HASH) {
                                num_increment_events_received += 1;
                                total_events_received += 1;
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                    break;
                }
            }
        }

        assert!(
            num_set_events_received == 1,
            "Number of set events received: {}",
            num_set_events_received
        );
        assert!(
            num_increment_events_received == 1,
            "Number of increment events received: {}",
            num_increment_events_received
        );
        assert!(total_events_received == 2, "Total events received: {}", total_events_received);

        drop(anvil);
    }

    fn get_wallet(anvil: &AnvilInstance) -> SeismicWallet<SeismicFoundry> {
        let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
        let wallet = SeismicWallet::from(bob.clone());
        wallet
    }
}
