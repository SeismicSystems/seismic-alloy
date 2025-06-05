//! Seismic provider for HTTP requests
use alloy_provider::{
    fillers::{FillProvider, JoinFill, RecommendedFillers, WalletFiller},
    Identity, Provider, ProviderBuilder, ProviderLayer, RootProvider,
};
use alloy_rpc_client::RpcClient;
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
    P: Provider<N>,
    N: SeismicNetwork,
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
    P: Provider<N>,
{
    fn root(&self) -> &RootProvider<N> {
        self.inner.root()
    }
}

impl<P> SeismicProviderExt<SeismicReth> for SeismicProvider<SeismicReth, P> where
    P: Provider<SeismicReth>
{
}
impl<P> SeismicProviderExt<SeismicFoundry> for SeismicProvider<SeismicFoundry, P> where
    P: Provider<SeismicFoundry>
{
}

/// Seismic layer
/// Consists of a SeismicProvider wrapping other layers
/// the SeismicProvider is responsible for encrypting and decrypting transactions
#[derive(Debug, Clone)]
pub(crate) struct SeismicLayer;

impl<N: SeismicNetwork, P> ProviderLayer<P, N> for SeismicLayer
where
    N::UnsignedTx: Send + Sync,
    P: Provider<N>,
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

impl<N: SeismicNetwork> Deref for SeismicSignedProvider<N>
where
    N::UnsignedTx: Send + Sync,
{
    type Target = SeismicSignedProviderInner<N>;

    fn deref(&self) -> &Self::Target {
        &self.0
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
{
    /// Creates a new seismic unsigned provider
    pub fn new(url: reqwest::Url) -> Self {
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
    SeismicUnsignedProvider::new(url)
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
    SeismicUnsignedProvider::new(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::ContractTestContext;
    use alloy_network::TransactionBuilder;
    use alloy_node_bindings::{Anvil, AnvilInstance};
    use alloy_primitives::{address, Address, Bytes, TxKind};
    use alloy_provider::{ext::AnvilApi, Provider, SendableTx};
    use alloy_signer_local::PrivateKeySigner;
    use seismic_alloy_network::foundry::builder::seismic_foundry_tx_builder;

    /// Path to local sanvil binary
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

        assert_eq!(tee_pubkey, seismic_enclave::crypto::get_unsecure_sample_secp256k1_pk());
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
        let provider = SeismicUnsignedProvider::<SeismicFoundry>::new(anvil.endpoint_url());

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
            SeismicUnsignedProvider::<SeismicFoundry>::new(anvil.endpoint_url());

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

        let tx =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

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
        let tx = seismic_foundry_tx_builder()
            .with_input(plaintext)
            .with_kind(TxKind::Create)
            .with_nonce(1)
            .into();

        let contract_address = provider
            .send_transaction(tx.into())
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

    fn get_wallet(anvil: &AnvilInstance) -> SeismicWallet<SeismicFoundry> {
        let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
        let wallet = SeismicWallet::from(bob.clone());
        wallet
    }
}
