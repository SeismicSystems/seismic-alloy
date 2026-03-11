//! Convenience constructors for Seismic providers.
//!
//! Signed providers encrypt calldata and decrypt responses. They require a
//! wallet for signing and fetch the TEE public key at creation time.
//!
//! Unsigned providers encrypt calldata but do not decrypt responses. They
//! don't need a wallet or TEE public key.
use alloy_provider::{
    fillers::{
        ChainIdFiller, FillProvider, JoinFill, NonceFiller, SimpleNonceManager, WalletFiller,
    },
    ProviderBuilder, RootProvider, WsConnect,
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

use crate::decrypt::{ResponseDecryptLayer, ResponseDecryptProvider};
use crate::SeismicProviderExt;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Filler chain for signed providers:
/// Wallet → (Nonce + ChainId) → SeismicElements → Gas
type SignedFillers<N> = JoinFill<
    JoinFill<
        JoinFill<WalletFiller<SeismicWallet<N>>, JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>>,
        SeismicElementsFiller,
    >,
    SeismicGasFiller,
>;

/// Filler chain for unsigned providers:
/// SeismicElements → (Nonce + ChainId) → Gas
type UnsignedFillers = JoinFill<
    JoinFill<SeismicElementsFiller, JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>>,
    SeismicGasFiller,
>;

/// A signed Seismic provider: decrypts responses, has a wallet for signing.
pub type SeismicSignedProvider<N> =
    ResponseDecryptProvider<N, FillProvider<SignedFillers<N>, RootProvider<N>, N>>;

/// An unsigned Seismic provider: encrypts calldata but does not decrypt responses.
pub type SeismicUnsignedProvider<N> = FillProvider<UnsignedFillers, RootProvider<N>, N>;

// ---------------------------------------------------------------------------
// Generic constructors
// ---------------------------------------------------------------------------

/// Build a signed provider that fetches the TEE public key automatically.
pub async fn signed_provider<N: SeismicNetwork>(
    wallet: impl Into<SeismicWallet<N>>,
    url: reqwest::Url,
) -> TransportResult<SeismicSignedProvider<N>>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    // Fetch TEE pubkey once using a temporary provider
    let temp_provider =
        ProviderBuilder::<_, _, N>::default().network::<N>().connect_client(RpcClient::new_http(url.clone()));
    let tee_pubkey = temp_provider.get_tee_pubkey().await?;

    Ok(signed_provider_with_tee_pubkey(wallet, url, tee_pubkey))
}

/// Build a signed provider with a pre-fetched TEE public key.
pub fn signed_provider_with_tee_pubkey<N: SeismicNetwork>(
    wallet: impl Into<SeismicWallet<N>>,
    url: reqwest::Url,
    tee_pubkey: seismic_enclave::secp256k1::PublicKey,
) -> SeismicSignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    let seismic_filler = SeismicElementsFiller::with_tee_pubkey_and_url(tee_pubkey);
    let ephemeral_secret_key = seismic_filler.ephemeral_secret_key().clone();

    let filler_chain = JoinFill::new(
        JoinFill::new(
            JoinFill::new(
                WalletFiller::new(wallet.into()),
                JoinFill::new(NonceFiller::<SimpleNonceManager>::simple(), ChainIdFiller::default()),
            ),
            seismic_filler,
        ),
        SeismicGasFiller::with_url(url.clone()),
    );

    ProviderBuilder::<_, _, N>::default()
        .network::<N>()
        .layer(ResponseDecryptLayer::new(ephemeral_secret_key, tee_pubkey))
        .layer(filler_chain)
        .connect_client(RpcClient::new_http(url))
}

/// Build an unsigned HTTP provider.
pub fn unsigned_provider_http<N: SeismicNetwork>(url: reqwest::Url) -> SeismicUnsignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    let filler_chain = unsigned_filler_chain(url.clone());

    ProviderBuilder::<_, _, N>::default()
        .network::<N>()
        .layer(filler_chain)
        .connect_client(RpcClient::new_http(url))
}

/// Build an unsigned WebSocket provider.
pub async fn unsigned_provider_ws<N: SeismicNetwork>(
    url: reqwest::Url,
) -> TransportResult<SeismicUnsignedProvider<N>>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    let filler_chain = unsigned_filler_chain(url.clone());

    ProviderBuilder::<_, _, N>::default()
        .network::<N>()
        .layer(filler_chain)
        .connect_ws(WsConnect::new(url))
        .await
}

/// Shared filler chain construction for unsigned providers.
fn unsigned_filler_chain(url: reqwest::Url) -> UnsignedFillers {
    JoinFill::new(
        JoinFill::new(
            SeismicElementsFiller::new(),
            JoinFill::new(NonceFiller::<SimpleNonceManager>::simple(), ChainIdFiller::default()),
        ),
        SeismicGasFiller::with_url(url),
    )
}

// ---------------------------------------------------------------------------
// Network-specific convenience functions
// ---------------------------------------------------------------------------

/// Create a signed provider for the SeismicFoundry (sanvil) network.
pub async fn sfoundry_signed_provider(
    wallet: impl Into<SeismicWallet<SeismicFoundry>>,
    url: reqwest::Url,
) -> TransportResult<SeismicSignedProvider<SeismicFoundry>> {
    signed_provider(wallet, url).await
}

/// Create an unsigned HTTP provider for the SeismicFoundry (sanvil) network.
pub fn sfoundry_unsigned_provider(url: reqwest::Url) -> SeismicUnsignedProvider<SeismicFoundry> {
    unsigned_provider_http(url)
}

/// Create a signed provider for the SeismicReth network.
pub async fn sreth_signed_provider(
    wallet: impl Into<SeismicWallet<SeismicReth>>,
    url: reqwest::Url,
) -> TransportResult<SeismicSignedProvider<SeismicReth>> {
    signed_provider(wallet, url).await
}

/// Create an unsigned HTTP provider for the SeismicReth network.
pub fn sreth_unsigned_provider(url: reqwest::Url) -> SeismicUnsignedProvider<SeismicReth> {
    unsigned_provider_http(url)
}
