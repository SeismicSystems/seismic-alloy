//! Seismic provider for HTTP requests
use alloy_network::TransactionBuilder;
use alloy_primitives::Bytes;
use alloy_provider::{
    fillers::{FillProvider, JoinFill, RecommendedFillers, WalletFiller},
    Identity, PendingTransactionBuilder, Provider, ProviderBuilder, ProviderLayer, RootProvider,
    SendableTx, WsConnect,
};
use alloy_rpc_client::RpcClient;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::{InputDecryptionElements, TxSeismicElements};
use seismic_alloy_network::{
    foundry::SeismicFoundry, seismic_network::SeismicNetwork, wallet::SeismicWallet, SeismicReth,
};
use std::ops::Deref;

use crate::SeismicProviderExt;

/// Seismic middleware for encrypting transactions and decrypting responses
/// Impliments [`SeismicProviderExt`] trait
#[derive(Debug, Clone)]
pub struct SeismicProvider<N, P> {
    /// Inner provider.
    inner: P,
    _network: std::marker::PhantomData<N>,
}

impl<N: SeismicNetwork, P> SeismicProvider<N, P>
where
    N::UnsignedTx: Send + Sync,
    P: SeismicProviderExt<N>,
{
    /// Create a new seismic provider
    pub(crate) fn new(inner: P) -> Self {
        Self { inner, _network: std::marker::PhantomData }
    }
}

/// Implement the Provider trait for the SeismicProvider
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N: SeismicNetwork, P> Provider<N> for SeismicProvider<N, P>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: InputDecryptionElements,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    fn root(&self) -> &RootProvider<N> {
        self.inner.root()
    }

    async fn send_transaction_internal(
        &self,
        mut tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        if let Some(mut builder) = tx.as_mut_builder() {
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

                // Set seismic elements first so metadata includes them
                N::set_seismic_elements(&mut builder, seismic_elements);

                // Get plaintext input before encrypting
                let plaintext_input = N::get_request_input(builder).unwrap();

                // Build metadata from the builder (now with seismic_elements set)
                let tx_metadata = builder.metadata().map_err(|e| {
                    TransportErrorKind::custom_str(&format!("Error building metadata: {:?}", e))
                })?;

                // Encrypt using the metadata
                let encrypted_input = seismic_elements
                    .client_encrypt(&plaintext_input, &network_pk, &encryption_keypair.secret_key(), &tx_metadata)
                    .map_err(|e| {
                        TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e))
                    })?;

                // Set the encrypted input
                N::set_request_input(builder, encrypted_input)
                    .map_err(|_| TransportErrorKind::custom_str("Error setting encrypted input"))?;
            }
        }
        let res = self.inner.send_transaction_internal(tx).await;
        res
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N: SeismicNetwork, P> SeismicProviderExt<N> for SeismicProvider<N, P>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: InputDecryptionElements,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Encrypts the input data, runs self.call_conditionally_signed, and decrypts the output data
    async fn seismic_call(&self, mut tx: SendableTx<N>) -> TransportResult<Bytes> {
        // set up elements unrelated to the input tx
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
        let (new_tx, tx_metadata) = match tx {
            SendableTx::Builder(mut builder) => {
                // Set seismic elements first
                N::set_seismic_elements(&mut builder, seismic_elements);

                let plaintext_input = N::get_request_input(&builder).unwrap();

                // Build metadata from builder (with seismic_elements set)
                let metadata = builder.metadata().map_err(|e| {
                    TransportErrorKind::custom_str(&format!("Error building metadata: {:?}", e))
                })?;

                let encrypted_input = seismic_elements
                    .client_encrypt(&plaintext_input, &network_pk, &encryption_keypair.secret_key(), &metadata)
                    .map_err(|e| {
                        TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e))
                    })?;

                TransactionBuilder::<N>::set_input(&mut builder, encrypted_input);
                (SendableTx::Builder(builder), metadata)
            }
            SendableTx::Envelope(_) => {
                return TransportResult::Err(
                    TransportErrorKind::custom_str(
                        "SeismicProvider::seismic_call does not support envelope transactions",
                    )
                    .into(),
                )
            }
        };
        tx = new_tx;

        // delegate to inner provider (e.g., FillProvider, RootProvider, etc.) and make rpc call
        let encrypted_output = self.inner.seismic_call(tx).await?;

        // decrypt the output
        let decrypted_output = seismic_elements
            .client_decrypt(&encrypted_output, &network_pk, &encryption_keypair.secret_key(), &tx_metadata)
            .map_err(|e| {
                TransportErrorKind::custom_str(&format!(
                    "Provider decryption error during seismic_call: {:?}. ciphertext: {:?}",
                    e, encrypted_output
                ))
            })?;

        return Ok(decrypted_output);
    }
}

/// Seismic layer, incorperating [`SeismicProviderExt`] functionality
/// Consists of a SeismicProvider wrapping other layers
#[derive(Debug, Clone)]
pub(crate) struct SeismicLayer;

impl<N: SeismicNetwork, P> ProviderLayer<P, N> for SeismicLayer
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: InputDecryptionElements,
    P: SeismicProviderExt<N>,
    RootProvider<N>: SeismicProviderExt<N>,
{
    type Provider = SeismicProvider<N, P>;

    fn layer(&self, inner: P) -> Self::Provider {
        SeismicProvider::new(inner)
    }
}

/// Type alias for the recommended fillers for the seismic network
/// Defined for code clarity
type SeismicRecFillers<N> = <N as RecommendedFillers>::RecommendedFillers;

/// Seismic provider type alias for signed provider
pub type SeismicSignedProviderInner<N> = SeismicProvider<
    N,
    FillProvider<
        JoinFill<SeismicRecFillers<N>, WalletFiller<SeismicWallet<N>>>,
        RootProvider<N>,
        N,
    >,
>;

/// Seismic signed provider
#[derive(Debug, Clone)]
pub struct SeismicSignedProvider<N: SeismicNetwork>(SeismicSignedProviderInner<N>)
where
    N::UnsignedTx: Send + Sync;

impl<N: SeismicNetwork> SeismicSignedProvider<N>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: InputDecryptionElements,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Creates a new seismic signed provider
    pub fn new(wallet: impl Into<SeismicWallet<N>>, url: reqwest::Url) -> Self {
        let tx_filler_layer = JoinFill::new(
            <N as RecommendedFillers>::recommended_fillers(),
            WalletFiller::new(wallet.into()),
        );

        // Build and return the provider
        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .connect_client(RpcClient::new_http(url));

        Self(inner)
    }
}

/// Seismic unsigned provider type alias
pub type SeismicUnsignedProviderInner<N> =
    SeismicProvider<N, FillProvider<JoinFill<Identity, SeismicRecFillers<N>>, RootProvider<N>, N>>;

/// Seismic unsigned provider
#[derive(Debug, Clone)]
pub struct SeismicUnsignedProvider<N: SeismicNetwork>(SeismicUnsignedProviderInner<N>)
where
    N::UnsignedTx: Send + Sync;

impl<N: SeismicNetwork> SeismicUnsignedProvider<N>
where
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Creates a new Seismic unsigned provider (defaults to HTTP connection via `new_http`)
    #[deprecated(note = "Use `new_http` instead")]
    pub fn new(url: reqwest::Url) -> Self
    where
        N::TransactionRequest: InputDecryptionElements,
    {
        Self::new_http(url)
    }

    /// Creates a new Seismic unsigned provider with an HTTP connection
    pub fn new_http(url: reqwest::Url) -> Self
    where
        N::TransactionRequest: InputDecryptionElements,
    {
        // Create layer with recommended fillers and Identity
        let tx_filler_layer =
            JoinFill::new(Identity, <N as RecommendedFillers>::recommended_fillers());

        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .connect_client(RpcClient::new_http(url));

        Self(inner)
    }

    /// Creates a new Seismic unsigned provider with a websocket connection
    pub async fn new_ws(url: reqwest::Url) -> Self
    where
        N::TransactionRequest: InputDecryptionElements,
    {
        let tx_filler_layer =
            JoinFill::new(Identity, <N as RecommendedFillers>::recommended_fillers());

        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .connect_ws(WsConnect::new(url))
            .await
            .unwrap();

        Self(inner)
    }
}

impl<N: SeismicNetwork, P> Deref for SeismicProvider<N, P>
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
    N::UnsignedTx: Send + Sync,
{
    type Target = SeismicSignedProviderInner<N>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<N: SeismicNetwork> Deref for SeismicUnsignedProvider<N>
where
    N::UnsignedTx: Send + Sync,
{
    type Target = SeismicUnsignedProviderInner<N>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Create a new SeismicSignedProvider for the SeismicReth network
pub fn sreth_signed_provider(
    wallet: impl Into<SeismicWallet<SeismicReth>>,
    url: reqwest::Url,
) -> SeismicSignedProvider<SeismicReth> {
    SeismicSignedProvider::new(wallet, url)
}

/// Create a new SeismicUnsignedProvider for the SeismicReth network
pub fn sreth_unsigned_provider(url: reqwest::Url) -> SeismicUnsignedProvider<SeismicReth> {
    SeismicUnsignedProvider::new_http(url)
}

/// Create a new SeismicSignedProvider for the SeismicFoundry network
pub fn sfoundry_signed_provider(
    wallet: impl Into<SeismicWallet<SeismicFoundry>>,
    url: reqwest::Url,
) -> SeismicSignedProvider<SeismicFoundry> {
    SeismicSignedProvider::new(wallet, url)
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
    use alloy_primitives::{address, Address, Bytes, TxKind, B256};
    use alloy_provider::{ext::AnvilApi, Provider, SendableTx};
    use alloy_rpc_types_eth::Filter;
    use alloy_signer_local::PrivateKeySigner;
    use alloy_sol_types::SolEvent;
    use futures_util::StreamExt;
    use seismic_alloy_consensus::{SeismicReceiptEnvelope, TxSeismic};
    use seismic_alloy_network::foundry::builder::seismic_foundry_tx_builder;

    /// Path to local sanvil binary for local testing
    const SANVIL_PATH: &str = "sanvil";

    #[tokio::test]
    async fn test_get_tee_pubkey() {
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url());

        // If this fails with a message like "Method Not Found", then you may be using anvil instead
        // of sanvil
        let tee_pubkey = provider.get_tee_pubkey().await.unwrap();

        assert_eq!(tee_pubkey, seismic_enclave::get_unsecure_sample_secp256k1_pk());
    }

    #[tokio::test]
    async fn test_send_transaction_with_empty_input() {
        let plaintext = Bytes::new();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url());

        let tx = seismic_foundry_tx_builder().with_input(plaintext).with_to(Address::ZERO).into();
        let res = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = res.get_receipt().await.unwrap();
        assert_eq!(receipt.inner.inner.status(), true);
    }

    /// Check that SeismicUnsignedProvider can inherit alloy_provider ext traits (and that they
    /// work)
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
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url());

        let mut tx =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        // Set valid seismic elements with security fields
        let elements = TxSeismicElements::default()
            .with_recent_block_hash(B256::from_slice(&[1u8; 32]))
            .with_expires_at_block(1000000)
            .with_signed_read(false);
        tx.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        tx.seismic_elements = Some(elements);

        let res = provider.seismic_call(SendableTx::Builder(tx.into())).await;
        assert!(res.is_ok(), "seismic_call failed: {:?}", res.unwrap_err());
        let res = res.unwrap();

        assert_eq!(res, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_send_transaction() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).spawn();
        let wallet = get_wallet(&anvil);
        let provider =
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url());

        // testing send transaction
        let tx =
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
            SeismicSignedProvider::<SeismicFoundry>::new(wallet.clone(), anvil.endpoint_url());

        // testing send transaction
        let tx =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();
        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());

        let network_pk = provider.get_tee_pubkey().await.unwrap();
        let encryption_keypair = TxSeismicElements::get_rand_encryption_keypair();
        let elements = TxSeismicElements::default()
            .with_encryption_pubkey(encryption_keypair.public_key())
            .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
            .with_recent_block_hash(B256::from_slice(&[1u8; 32]))
            .with_expires_at_block(1000000)
            .with_signed_read(false);

        let tx_input = ContractTestContext::get_set_number_input_plaintext();
        let encrypted_input = elements
            .client_encrypt(&tx_input, &network_pk, &encryption_keypair.secret_key())
            .unwrap();

        let mut tx = seismic_foundry_tx_builder()
            .with_input(encrypted_input)
            .with_kind(TxKind::Call(contract_address))
            .into();
        tx.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        tx.seismic_elements = Some(elements);

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
        let provider = SeismicSignedProvider::<SeismicFoundry>::new(wallet, anvil.endpoint_url());
        let ws_provider =
            SeismicUnsignedProvider::<SeismicFoundry>::new_ws(anvil.ws_endpoint_url()).await;

        // deploy contract
        let tx =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();
        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();
        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());
        let filter = Filter::new().address(contract_address);

        // subscribe to events
        let event_sub = ws_provider.subscribe_logs(&filter).await.unwrap();

        // set number
        let network_pk = provider.get_tee_pubkey().await.unwrap();
        let encryption_keypair = TxSeismicElements::get_rand_encryption_keypair();
        let elements = TxSeismicElements::default()
            .with_encryption_pubkey(encryption_keypair.public_key())
            .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
            .with_recent_block_hash(B256::from_slice(&[1u8; 32]))
            .with_expires_at_block(1000000)
            .with_signed_read(false);

        let tx_input_set_number = ContractTestContext::get_set_number_input_plaintext();
        let encrypted_input = elements
            .client_encrypt(&tx_input_set_number, &network_pk, &encryption_keypair.secret_key())
            .unwrap();

        let mut tx_set_number = seismic_foundry_tx_builder()
            .with_input(encrypted_input)
            .with_kind(TxKind::Call(contract_address))
            .into();
        tx_set_number.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        tx_set_number.seismic_elements = Some(elements);

        let pending_tx_set_number = provider.send_transaction(tx_set_number.into()).await.unwrap();
        let receipt_set_number = pending_tx_set_number.get_receipt().await.unwrap();

        assert!(receipt_set_number.status());

        // increment number
        let tx_input_increment_number = ContractTestContext::get_increment_input_plaintext();
        let encrypted_input = elements
            .client_encrypt(
                &tx_input_increment_number,
                &network_pk,
                &encryption_keypair.secret_key(),
            )
            .unwrap();

        let mut tx_increment_number = seismic_foundry_tx_builder()
            .with_input(encrypted_input)
            .with_kind(TxKind::Call(contract_address))
            .into();
        tx_increment_number.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        tx_increment_number.seismic_elements = Some(elements);

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
