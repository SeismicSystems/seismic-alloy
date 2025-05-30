//! Seismic provider for HTTP requests
use alloy_provider::{
    builder, fillers::{
        BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
        RecommendedFillers, SimpleNonceManager, WalletFiller,
    }, Identity, PendingTransactionBuilder, Provider, ProviderBuilder, ProviderCall, ProviderLayer, RootProvider, SendableTx
};
use alloy_network::{Ethereum, EthereumWallet};
use alloy_rpc_client::NoParams;
use seismic_alloy_consensus::TxSeismicElements;
use seismic_alloy_network::Seismic;
use seismic_enclave::PublicKey;
use std::ops::Deref;
use alloy_network::{TransactionBuilder};
use alloy_primitives::Bytes;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

/// Seismic middlware for encrypting transactions and decrypting responses
#[derive(Debug, Clone)]
pub struct SeismicProvider<P> {
    /// Inner provider.
    inner: P,
}

impl<P> SeismicProvider<P>
where
    P: Provider<Seismic>,
{
    /// Create a new seismic provider
    pub(crate) fn new(inner: P) -> Self {
        Self { inner }
    }

    /// Should encrypt input
    pub(crate) fn should_encrypt_input<B: TransactionBuilder<Seismic>>(&self, tx: &B) -> bool {
        tx.input().map_or(false, |input| !input.is_empty())
    }

    fn get_tee_pubkey(&self) -> ProviderCall<NoParams, PublicKey> {
        self.client().request_noparams("seismic_getTeePublicKey").into()
    }
}

impl<P> SeismicProvider<P>
where
    P: Provider<Seismic>,
{
    async fn seismic_call(&self, mut tx: SendableTx<Seismic>) -> TransportResult<Bytes> {
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

                // decrypting output
                return self
                    .inner
                    .call(builder.clone())
                    .await
                    .and_then(|encrypted_output| {
                        // Decrypt the output using the encryption keypair
                        let decrypted_output = seismic_elements
                            .client_decrypt(
                                &encrypted_output,
                                &network_pk,
                                &encryption_keypair.secret_key(),
                            )
                            .map_err(|e| {
                                TransportErrorKind::custom_str(&format!(
                                    "Error decrypting output: {:?}",
                                    e
                                ))
                            })?;
                        Ok(Bytes::from(decrypted_output))
                    });
            }
        }
        match tx {
            SendableTx::Builder(builder) => {
                self.inner.call(builder.clone()).await
            }
            SendableTx::Envelope(envelope) => {
                self.inner.call(envelope.into()).await
            }
        }
    }
}

/// Implement the Provider trait for the SeismicProvider
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<P> Provider<Seismic> for SeismicProvider<P>
where
    P: Provider<Seismic>,
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

/// Seismic layer
#[derive(Debug, Clone)]
pub(crate) struct SeismicLayer {}

impl<P> ProviderLayer<P> for SeismicLayer
where
    P: Provider<Seismic>,
{
    type Provider = SeismicProvider<P>;

    fn layer(&self, inner: P) -> Self::Provider {
        SeismicProvider::new(inner)
    }
}

/// Seismic provider
pub type SeismicSignedProviderInner = SeismicProvider<
    FillProvider<
        JoinFill<
            JoinFill<
                GasFiller,
                JoinFill<BlobGasFiller, JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>>,
            >,
            WalletFiller<EthereumWallet>,
        >,
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
        // Create wallet layer with recommended fillers
        let tx_filler_layer = JoinFill::new(
            JoinFill::new(
                GasFiller,
                JoinFill::new(
                    BlobGasFiller,
                    JoinFill::new(
                        NonceFiller::<SimpleNonceManager>::default(),
                        ChainIdFiller::default(),
                    ),
                ),
            ),
            WalletFiller::new(wallet.clone()),
        );

        // Build and return the provider
        let inner = ProviderBuilder::new()
            .network::<Seismic>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .on_http(url);
        Self(inner)
    }
}
impl Deref for SeismicSignedProvider {
    type Target = SeismicSignedProviderInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Seismic unsigned provider
pub type SeismicUnsignedProviderInner = SeismicProvider<
    FillProvider<
        JoinFill<<Ethereum as RecommendedFillers>::RecommendedFillers, Identity>,
        RootProvider<Seismic>,
        Ethereum,
    >
>;

/// Seismic unsigned provider
#[derive(Debug, Clone)]
pub struct SeismicUnsignedProvider(SeismicUnsignedProviderInner);

impl SeismicUnsignedProvider {
    /// Creates a new seismic unsigned provider
    pub fn new(url: reqwest::Url) -> Self {
        // Create wallet layer with recommended fillers
        let tx_filler_layer = JoinFill::new(Ethereum::recommended_fillers(), Identity);

        let inner = ProviderBuilder::new()
            .network::<Seismic>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .on_http(url);
        Self(inner)
    }
}

impl Deref for SeismicUnsignedProvider {
    type Target = SeismicUnsignedProviderInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/*
#[cfg(test)]
mod tests {
    use alloy_network::{Ethereum, EthereumWallet, TransactionBuilder};
    use alloy_node_bindings::{Anvil, AnvilInstance};
    use alloy_primitives::{Address, Bytes, TxKind};
    use alloy_rpc_types_eth::TransactionRequest;
    use alloy_signer_local::PrivateKeySigner;

    use crate::{
        layers::seismic::{layer::SeismicLayer, test_utils::ContractTestContext},
        Provider, ProviderBuilder, SeismicSignedProvider, SeismicUnsignedProvider, SendableTx,
    };

    #[tokio::test]
    async fn test_seismic_signed_call() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::new().spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        let tx = TransactionRequest::default().with_input(plaintext).with_kind(TxKind::Create);

        let res = provider.seismic_call(SendableTx::Builder(tx)).await.unwrap();

        assert_eq!(res, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_seismic_unsigned_call() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::new().spawn();
        let from = get_wallet(&anvil).default_signer().address();
        let unsigned_provider = SeismicUnsignedProvider::new(anvil.endpoint_url());

        let tx = TransactionRequest::default()
            .with_input(plaintext)
            .with_kind(TxKind::Create)
            .with_from(from);

        let res = unsigned_provider.seismic_call(SendableTx::Builder(tx)).await.unwrap();
        assert_eq!(res, ContractTestContext::get_code());
    }

    #[tokio::test]
    async fn test_send_transaction() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::new().spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        // testing send transaction
        let tx = TransactionRequest::default().with_input(plaintext).with_kind(TxKind::Create);

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

    #[tokio::test]
    async fn test_send_transaction_with_emtpy_input() {
        let plaintext = Bytes::new();
        let anvil = Anvil::new().spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::new(wallet.clone(), anvil.endpoint_url());

        let tx = TransactionRequest::default().with_input(plaintext).with_to(Address::ZERO);

        let res = provider.send_transaction(tx).await.unwrap();
        let receipt = res.get_receipt().await.unwrap();
        assert_eq!(receipt.status(), true);
    }

    #[tokio::test]
    async fn test_get_tee_pubkey() {
        let provider =
            ProviderBuilder::new().network::<Seismic>().layer(SeismicLayer {}).on_anvil();
        let tee_pubkey = provider.get_tee_pubkey().await.unwrap();
        println!("test_get_tee_pubkey: tee_pubkey: {:?}", tee_pubkey);
    }

    fn get_wallet(anvil: &AnvilInstance) -> EthereumWallet {
        let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
        let wallet = EthereumWallet::from(bob.clone());
        wallet
    }
} 
*/