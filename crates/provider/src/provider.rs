//! Seismic provider for HTTP requests
use crate::SeismicProviderExt;
use alloy_network::{EthereumWallet, TransactionBuilder};
use alloy_primitives::Bytes;
use alloy_provider::{
    fillers::{FillProvider, JoinFill, RecommendedFillers, WalletFiller},
    Identity, PendingTransactionBuilder, Provider, ProviderBuilder, ProviderLayer, RootProvider,
    SendableTx,
};
use alloy_rpc_client::RpcClient;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::TxSeismicElements;
use seismic_alloy_network::Seismic;
use std::ops::Deref;

/// Seismic middleware for encrypting transactions and decrypting responses
/// Impliments [`SeismicProviderExt`] trait
#[derive(Debug, Clone)]
pub struct SeismicProvider<P> {
    /// Inner provider.
    inner: P,
}

impl<P> SeismicProvider<P>
where
    P: SeismicProviderExt,
{
    /// Create a new seismic provider
    pub(crate) fn new(inner: P) -> Self {
        Self { inner }
    }
}

/// Implement the Provider trait for the SeismicProvider
/// Overriding the send_transaction_internal method to encrypt and decrypt transactions
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<P> Provider<Seismic> for SeismicProvider<P>
where
    P: SeismicProviderExt,
{
    fn root(&self) -> &RootProvider<Seismic> {
        self.inner.root()
    }

    async fn send_transaction_internal(
        &self,
        mut tx: SendableTx<Seismic>,
    ) -> TransportResult<PendingTransactionBuilder<Seismic>> {
        if let Some(builder) = tx.as_mut_builder() {
            if self.should_encrypt_input(builder) {
                let network_pk = self.get_tee_pubkey().await.map_err(|e| {
                    TransportErrorKind::custom_str(&format!(
                        "Error getting tee pubkey from server: {:?}",
                        e
                    ))
                })?;
                let encryption_keypair = TxSeismicElements::get_rand_encryption_keypair();
                let seismic_elements = TxSeismicElements::default()
                    .with_encryption_pubkey(encryption_keypair.public_key())
                    .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce());

                // Encrypt using recipient's public key and generated private key
                let plaintext_input = builder.inner.input.input().unwrap();
                let encrypted_input = seismic_elements
                    .client_encrypt(&plaintext_input, &network_pk, &encryption_keypair.secret_key())
                    .map_err(|e| {
                        TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e))
                    })?;

                builder.set_input(Bytes::from(encrypted_input));
                builder.set_seismic_elements(seismic_elements);
            }
        }
        let res = self.inner.send_transaction_internal(tx).await;
        res
    }
}

#[async_trait::async_trait]
impl<P> SeismicProviderExt for SeismicProvider<P>
where
    P: SeismicProviderExt,
{
    async fn seismic_call(&self, tx: SendableTx<Seismic>) -> TransportResult<Bytes> {
        // delegate to inner provider, ex a FillProvider, RootProvider, etc
        self.inner.seismic_call(tx).await
    }
}

/// Seismic layer, incorperating [`SeismicProviderExt`] functionality
/// Consists of a SeismicProvider wrapping other layers
#[derive(Debug, Clone)]
pub(crate) struct SeismicLayer;

impl<P> ProviderLayer<P, Seismic> for SeismicLayer
where
    P: SeismicProviderExt,
{
    type Provider = SeismicProvider<P>;

    fn layer(&self, inner: P) -> Self::Provider {
        SeismicProvider::new(inner)
    }
}

/// Type alias for the recommended fillers for the seismic network
/// Defined for code clarity
type SeismicRecFillers = <seismic_alloy_network::Seismic as RecommendedFillers>::RecommendedFillers;

/// Seismic provider type alias for signed provider
pub type SeismicSignedProviderInner = SeismicProvider<
    FillProvider<
        JoinFill<SeismicRecFillers, WalletFiller<EthereumWallet>>,
        RootProvider<Seismic>,
        Seismic,
    >,
>;

/// Seismic signed provider
#[derive(Debug, Clone)]
pub struct SeismicSignedProvider(SeismicSignedProviderInner);

impl SeismicSignedProvider {
    /// Creates a new seismic signed provider
    pub fn new(wallet: EthereumWallet, url: reqwest::Url) -> Self {
        let tx_filler_layer = JoinFill::new(
            <Seismic as RecommendedFillers>::recommended_fillers(),
            WalletFiller::new(wallet.clone()),
        );

        // Build and return the provider
        let inner = ProviderBuilder::<_, _, Seismic>::default()
            .network::<Seismic>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .on_client(RpcClient::new_http(url));

        Self(inner)
    }
}

impl Deref for SeismicSignedProvider {
    type Target = SeismicSignedProviderInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Seismic unsigned provider type alias
/// Defined for code clarity
pub type SeismicUnsignedProviderInner = SeismicProvider<
    FillProvider<JoinFill<Identity, SeismicRecFillers>, RootProvider<Seismic>, Seismic>,
>;

/// Seismic unsigned provider
#[derive(Debug, Clone)]
pub struct SeismicUnsignedProvider(SeismicUnsignedProviderInner);

impl SeismicUnsignedProvider {
    /// Creates a new seismic unsigned provider
    pub fn new(url: reqwest::Url) -> Self {
        // Create layer with recommended fillers and Identity
        let tx_filler_layer =
            JoinFill::new(Identity, <Seismic as RecommendedFillers>::recommended_fillers());

        let inner = ProviderBuilder::<_, _, Seismic>::default()
            .network::<Seismic>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .on_client(RpcClient::new_http(url));

        Self(inner)
    }
}

impl Deref for SeismicUnsignedProvider {
    type Target = SeismicUnsignedProviderInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::ContractTestContext;
    use alloy_network::{EthereumWallet, TransactionBuilder};
    use alloy_node_bindings::{Anvil, AnvilInstance};
    use alloy_primitives::{address, Address, Bytes, TxKind};
    use alloy_provider::{ext::AnvilApi, Provider, SendableTx};
    use alloy_signer_local::PrivateKeySigner;
    use seismic_alloy_rpc_types::SeismicTransactionRequest;

    /// Path to local sanvil binary
    const SANVIL_PATH: &str = "sanvil";

    #[tokio::test]
    async fn test_get_tee_pubkey() {
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        // If this fails with a message like "Method Not Found", then you may be using anvil instead
        // of sanvil
        let tee_pubkey = provider.get_tee_pubkey().await.unwrap();

        assert_eq!(tee_pubkey, seismic_enclave::crypto::get_unsecure_sample_secp256k1_pk());
    }

    #[tokio::test]
    async fn test_send_transaction_with_emtpy_input() {
        let plaintext = Bytes::new();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        let tx = SeismicTransactionRequest::default().with_input(plaintext).with_to(Address::ZERO);

        let res = provider.send_transaction(tx).await.unwrap();
        let receipt = res.get_receipt().await.unwrap();
        assert_eq!(receipt.inner.status(), true);
    }

    /// Check that SeismicUnsignedProvider can inherit alloy_provider ext traits (and that they
    /// work)
    #[tokio::test]
    async fn test_anvil_set_code() {
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let provider = SeismicUnsignedProvider::new(anvil.endpoint_url());

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
        let unsigned_provider = SeismicUnsignedProvider::new(anvil.endpoint_url());

        let tx = SeismicTransactionRequest::default()
            .with_input(plaintext)
            .with_kind(TxKind::Create)
            .with_from(from);

        let res = unsigned_provider.seismic_call(SendableTx::Builder(tx)).await.unwrap();
        assert_eq!(res, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_seismic_signed_call() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        let tx =
            SeismicTransactionRequest::default().with_input(plaintext).with_kind(TxKind::Create);

        let res = provider.seismic_call(SendableTx::Builder(tx)).await;
        assert!(res.is_ok(), "seismic_call failed: {:?}", res.unwrap_err());
        let res = res.unwrap();

        assert_eq!(res, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_send_transaction() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        // testing send transaction
        let tx = SeismicTransactionRequest::default()
            .with_input(plaintext)
            .with_kind(TxKind::Create)
            .with_nonce(1);

        let contract_address = provider
            .send_transaction(tx)
            .await
            .unwrap()
            .get_receipt()
            .await
            .unwrap()
            .contract_address
            .unwrap();

        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());
    }

    fn get_wallet(anvil: &AnvilInstance) -> EthereumWallet {
        let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
        let wallet = EthereumWallet::from(bob.clone());
        wallet
    }
}
