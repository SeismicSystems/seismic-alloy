//! Builder for creating Seismic providers.
//!
//! Follows alloy's `ProviderBuilder` pattern. Defaults to `SeismicReth`
//! (production); use `.foundry()` for testing with sanvil.
//!
//! ```rust,ignore
//! // Production (SeismicReth, the default)
//! let provider = SeismicProviderBuilder::new()
//!     .wallet(wallet)
//!     .connect_http(url)
//!     .await?;
//!
//! // WebSocket (signed)
//! let provider = SeismicProviderBuilder::new()
//!     .wallet(wallet)
//!     .connect_ws(url)
//!     .await?;
//!
//! // Testing with sanvil (SeismicFoundry)
//! let provider = SeismicProviderBuilder::new()
//!     .foundry()
//!     .wallet(wallet)
//!     .connect_http(url)
//!     .await?;
//!
//! // Unsigned provider (no wallet, no response decryption)
//! let provider = SeismicProviderBuilder::new()
//!     .connect_http(url);
//! ```
use alloy_provider::{
    fillers::{
        ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, SimpleNonceManager,
        WalletFiller,
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

use crate::{
    decrypt::{ResponseDecryptLayer, ResponseDecryptProvider},
    SeismicProviderExt,
};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Filler chain for signed providers:
/// Wallet → (Nonce + ChainId) → SeismicElements → Gas
type SignedFillers<N> = JoinFill<
    JoinFill<
        JoinFill<
            WalletFiller<SeismicWallet<N>>,
            JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>,
        >,
        SeismicElementsFiller,
    >,
    SeismicGasFiller,
>;

/// Filler chain for unsigned providers:
/// (Nonce + ChainId) → Gas
type UnsignedFillers =
    JoinFill<JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>, GasFiller>;

/// A signed Seismic provider: decrypts responses, has a wallet for signing.
pub type SeismicSignedProvider<N> =
    ResponseDecryptProvider<N, FillProvider<SignedFillers<N>, RootProvider<N>, N>>;

/// An unsigned Seismic provider: standard (unencrypted) reads and writes only.
pub type SeismicUnsignedProvider<N> = FillProvider<UnsignedFillers, RootProvider<N>, N>;

// ---------------------------------------------------------------------------
// Builder — entry point, defaults to SeismicReth
// ---------------------------------------------------------------------------

/// Builder for creating Seismic providers.
///
/// Defaults to [`SeismicReth`] (production). Use `.foundry()` for sanvil testing
/// or `.network::<N>()` for a custom network.
///
/// ```rust,ignore
/// // Production (default)
/// let provider = SeismicProviderBuilder::new()
///     .wallet(wallet)
///     .connect_http(url)
///     .await?;
///
/// // Testing
/// let provider = SeismicProviderBuilder::new()
///     .foundry()
///     .wallet(wallet)
///     .connect_http(url)
///     .await?;
/// ```
#[derive(Debug)]
pub struct SeismicProviderBuilder;

impl SeismicProviderBuilder {
    /// Create a new builder. Defaults to [`SeismicReth`] network.
    pub fn new() -> Self {
        Self
    }

    /// Switch to [`SeismicFoundry`] network for testing with sanvil.
    pub fn foundry(self) -> SeismicProviderBuilderWithNetwork<SeismicFoundry> {
        self.network::<SeismicFoundry>()
    }

    /// Select a custom network.
    pub fn network<N: SeismicNetwork>(self) -> SeismicProviderBuilderWithNetwork<N>
    where
        N::TransactionRequest: AsRef<SeismicTransactionRequest>
            + AsMut<SeismicTransactionRequest>
            + From<SeismicTransactionRequest>
            + InputDecryptionElements,
        N::UnsignedTx: Send + Sync,
        RootProvider<N>: SeismicProviderExt<N>,
    {
        SeismicProviderBuilderWithNetwork { _network: std::marker::PhantomData }
    }

    /// Add a wallet for signing. Uses the default [`SeismicReth`] network.
    pub fn wallet(
        self,
        wallet: impl Into<SeismicWallet<SeismicReth>>,
    ) -> SeismicProviderBuilderWithWallet<SeismicReth> {
        SeismicProviderBuilderWithWallet {
            wallet: wallet.into(),
            _network: std::marker::PhantomData,
        }
    }

    /// Connect via HTTP as an unsigned provider. Uses the default [`SeismicReth`] network.
    pub fn connect_http(self, url: reqwest::Url) -> SeismicUnsignedProvider<SeismicReth> {
        build_unsigned_http(url)
    }

    /// Connect via WebSocket as an unsigned provider. Uses the default [`SeismicReth`] network.
    pub async fn connect_ws(
        self,
        url: reqwest::Url,
    ) -> TransportResult<SeismicUnsignedProvider<SeismicReth>> {
        build_unsigned_ws(url).await
    }
}

impl Default for SeismicProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Builder — network selected (non-default)
// ---------------------------------------------------------------------------

/// Builder state after selecting a non-default network.
#[derive(Debug)]
pub struct SeismicProviderBuilderWithNetwork<N: SeismicNetwork>
where
    N::UnsignedTx: Send + Sync,
{
    _network: std::marker::PhantomData<N>,
}

impl<N: SeismicNetwork> SeismicProviderBuilderWithNetwork<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Add a wallet for signing.
    pub fn wallet(
        self,
        wallet: impl Into<SeismicWallet<N>>,
    ) -> SeismicProviderBuilderWithWallet<N> {
        SeismicProviderBuilderWithWallet {
            wallet: wallet.into(),
            _network: std::marker::PhantomData,
        }
    }

    /// Connect via HTTP (unsigned provider).
    pub fn connect_http(self, url: reqwest::Url) -> SeismicUnsignedProvider<N> {
        build_unsigned_http(url)
    }

    /// Connect via WebSocket (unsigned provider).
    pub async fn connect_ws(
        self,
        url: reqwest::Url,
    ) -> TransportResult<SeismicUnsignedProvider<N>> {
        build_unsigned_ws(url).await
    }
}

// ---------------------------------------------------------------------------
// Builder — network + wallet selected
// ---------------------------------------------------------------------------

/// Builder state with network and wallet. Ready to connect as a signed provider.
#[derive(Debug)]
pub struct SeismicProviderBuilderWithWallet<N: SeismicNetwork>
where
    N::UnsignedTx: Send + Sync,
{
    wallet: SeismicWallet<N>,
    _network: std::marker::PhantomData<N>,
}

impl<N: SeismicNetwork> SeismicProviderBuilderWithWallet<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Connect via HTTP. Fetches the TEE public key automatically.
    pub async fn connect_http(
        self,
        url: reqwest::Url,
    ) -> TransportResult<SeismicSignedProvider<N>> {
        let temp_provider = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .connect_client(RpcClient::new_http(url.clone()));
        let tee_pubkey = temp_provider.get_tee_pubkey().await?;

        Ok(self.connect_http_with_tee_pubkey(url, tee_pubkey))
    }

    /// Connect via HTTP with a pre-fetched TEE public key (synchronous).
    pub fn connect_http_with_tee_pubkey(
        self,
        url: reqwest::Url,
        tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    ) -> SeismicSignedProvider<N> {
        let (filler_chain, ephemeral_secret_key) =
            self.signed_filler_chain(url.clone(), tee_pubkey);

        ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(ResponseDecryptLayer::new(ephemeral_secret_key, tee_pubkey))
            .layer(filler_chain)
            .connect_client(RpcClient::new_http(url))
    }

    /// Connect via WebSocket. Fetches the TEE public key automatically.
    pub async fn connect_ws(self, url: reqwest::Url) -> TransportResult<SeismicSignedProvider<N>> {
        let temp_provider = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .connect_ws(WsConnect::new(url.clone()))
            .await?;
        let tee_pubkey = temp_provider.get_tee_pubkey().await?;

        self.connect_ws_with_tee_pubkey(url, tee_pubkey).await
    }

    /// Connect via WebSocket with a pre-fetched TEE public key.
    pub async fn connect_ws_with_tee_pubkey(
        self,
        url: reqwest::Url,
        tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    ) -> TransportResult<SeismicSignedProvider<N>> {
        let (filler_chain, ephemeral_secret_key) =
            self.signed_filler_chain(url.clone(), tee_pubkey);

        ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(ResponseDecryptLayer::new(ephemeral_secret_key, tee_pubkey))
            .layer(filler_chain)
            .connect_ws(WsConnect::new(url))
            .await
    }

    /// Build the signed filler chain. Returns the chain and the ephemeral secret key
    /// (needed by ResponseDecryptLayer).
    fn signed_filler_chain(
        self,
        url: reqwest::Url,
        tee_pubkey: seismic_enclave::secp256k1::PublicKey,
    ) -> (SignedFillers<N>, seismic_enclave::secp256k1::SecretKey) {
        let seismic_filler = SeismicElementsFiller::with_tee_pubkey_and_url(tee_pubkey);
        let ephemeral_secret_key = seismic_filler.ephemeral_secret_key().clone();

        let filler_chain = JoinFill::new(
            JoinFill::new(
                JoinFill::new(
                    WalletFiller::new(self.wallet),
                    JoinFill::new(
                        NonceFiller::<SimpleNonceManager>::simple(),
                        ChainIdFiller::default(),
                    ),
                ),
                seismic_filler,
            ),
            SeismicGasFiller::with_url(url),
        );

        (filler_chain, ephemeral_secret_key)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn build_unsigned_http<N: SeismicNetwork>(url: reqwest::Url) -> SeismicUnsignedProvider<N>
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    let filler_chain = unsigned_filler_chain();

    ProviderBuilder::<_, _, N>::default()
        .network::<N>()
        .layer(filler_chain)
        .connect_client(RpcClient::new_http(url))
}

async fn build_unsigned_ws<N: SeismicNetwork>(
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
    let filler_chain = unsigned_filler_chain();

    ProviderBuilder::<_, _, N>::default()
        .network::<N>()
        .layer(filler_chain)
        .connect_ws(WsConnect::new(url))
        .await
}

fn unsigned_filler_chain() -> UnsignedFillers {
    JoinFill::new(
        JoinFill::new(NonceFiller::<SimpleNonceManager>::simple(), ChainIdFiller::default()),
        GasFiller::default(),
    )
}
