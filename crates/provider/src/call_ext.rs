//! Extension traits for seismic (encrypted) contract calls.
//!
//! Two traits provide the seismic call interface:
//!
//! - [`SeismicCallExt`] — adds `.seismic()` to alloy's `SolCallBuilder`, converting it to a
//!   [`ShieldedCallBuilder`] that encrypts calldata.
//! - [`ShieldedCallExt`] — provides `.call()`, `.send()`, and builder methods (`.expires_at()`,
//!   `.eip712()`, etc.) on [`ShieldedCallBuilder`].
//!
//! Functions with shielded parameters (e.g., `suint256`, `saddress`) are automatically
//! wrapped in `ShieldedCallBuilder` by the `sol!` macro, so `.call()` and `.send()`
//! auto-encrypt without needing `.seismic()`.
//!
//! ```rust,ignore
//! sol! {
//!     #[sol(rpc)]
//!     interface MyContract {
//!         function getPublicValue() public view returns (uint256);
//!         function setSecret(suint256 val) public;
//!     }
//! }
//!
//! let contract = MyContract::new(address, &provider);
//!
//! // Non-shielded function — opt in to encryption with .seismic()
//! let val = contract.getPublicValue().seismic().call().await?;
//!
//! // Shielded function — auto-encrypts, .seismic() is unnecessary
//! contract.setSecret(val).send().await?;
//!
//! // Transparent (default alloy behavior, works on any provider)
//! let val = contract.getPublicValue().call().await?;
//! ```
use alloy_contract::SolCallBuilder;
use alloy_network::{Network, TransactionBuilder};
use alloy_primitives::{aliases::U96, Address, Bytes, B256};
use alloy_provider::{PendingTransactionBuilder, Provider, SendableTx};
use alloy_sol_types::{private::ShieldedCallBuilder, SolCall};
use alloy_transport::TransportResult;

use crate::{SeismicProviderError, SeismicProviderExt};
use seismic_alloy_consensus::TxSeismicElements;
use seismic_alloy_network::seismic_network::SeismicNetwork;
use seismic_alloy_rpc_types::SeismicTransactionRequest;

// ============================================================================
// SignedProviderExt — sealed trait for signed providers
// ============================================================================

/// Sealed trait for signed providers that can encrypt calldata and decrypt responses.
///
/// Only implemented by [`ResponseDecryptProvider`] and references to it.
/// Provides the low-level `seismic_call` and `eip712_send` methods, plus
/// high-level `shielded_call` and `shielded_send` convenience methods.
///
/// Users typically don't interact with this trait directly — use the
/// call-builder traits ([`SeismicCallExt`], [`ShieldedCallExt`]) instead.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SignedProviderExt<N: SeismicNetwork>: SeismicProviderExt<N>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: From<SeismicTransactionRequest>,
{
    /// Low-level seismic call. Fills the transaction, sends as `eth_call`,
    /// and decrypts the response.
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes>;

    /// Send an EIP-712 signed seismic transaction.
    async fn eip712_send(&self, tx: SendableTx<N>)
        -> TransportResult<PendingTransactionBuilder<N>>;

    /// Encrypted, signed read call. Encrypts calldata, signs the call
    /// (preventing `msg.sender` spoofing), and decrypts the response.
    async fn shielded_call<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        let encoded = call.abi_encode();
        let mut tx: N::TransactionRequest =
            SeismicTransactionRequest::default().to(address).seismic().into();
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        let result = self.seismic_call(SendableTx::Builder(tx)).await?;

        C::abi_decode_returns(&result)
            .map_err(|e| SeismicProviderError::AbiDecode(e).into_transport())
    }

    /// Encrypted write transaction. The filler pipeline handles encryption key
    /// generation, nonce, gas estimation, and signing.
    async fn shielded_send<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        let encoded = call.abi_encode();
        let mut tx: N::TransactionRequest =
            SeismicTransactionRequest::default().to(address).seismic().into();
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        self.send_transaction(tx).await
    }
}

/// Blanket impl so `&T: SignedProviderExt` when `T: SignedProviderExt`.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<T, N> SignedProviderExt<N> for &T
where
    T: SignedProviderExt<N> + Sync,
    N: SeismicNetwork,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
    Self: SeismicProviderExt<N>,
{
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        (**self).seismic_call(tx).await
    }

    async fn eip712_send(
        &self,
        tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        (**self).eip712_send(tx).await
    }
}

// ============================================================================
// SeismicCallExt — .seismic() on SolCallBuilder
// ============================================================================

/// Extension trait that adds `.seismic()` to alloy's [`SolCallBuilder`].
///
/// Converts a `SolCallBuilder` into a [`ShieldedCallBuilder`] that routes
/// calls through the seismic (encrypted) path.
///
/// Only available on signed providers (those constructed with `.wallet()`).
pub trait SeismicCallExt<'a, P, C: SolCall, N: Network> {
    /// Mark this contract call as a seismic (encrypted) operation.
    ///
    /// Returns a [`ShieldedCallBuilder`] with `.call()`, `.send()`, and
    /// builder methods for customizing security parameters.
    fn seismic(self) -> ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>>;
}

impl<'a, P, C, N> SeismicCallExt<'a, P, C, N> for SolCallBuilder<&'a P, C, N>
where
    N: SeismicNetwork,
    C: SolCall,
    P: SignedProviderExt<N>,
    N::TransactionRequest: AsMut<SeismicTransactionRequest> + From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    fn seismic(self) -> ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>> {
        ShieldedCallBuilder(self)
    }
}

// ============================================================================
// ShieldedCallExt — .call(), .send(), builder methods on ShieldedCallBuilder
// ============================================================================

/// Extension trait for [`ShieldedCallBuilder`] — provides `.call()`, `.send()`,
/// and builder methods for seismic (encrypted) contract calls.
///
/// Functions with shielded types in their arguments are automatically wrapped
/// in `ShieldedCallBuilder` by the `sol!` macro, so `.call()` and `.send()`
/// auto-encrypt. Calling `.seismic()` first is unnecessary and produces a
/// deprecation warning.
///
/// For non-shielded functions, use [`SeismicCallExt::seismic()`] to convert
/// a `SolCallBuilder` into a `ShieldedCallBuilder` first.
pub trait ShieldedCallExt<'a, P, C: SolCall, N: Network> {
    /// Execute an encrypted read call.
    ///
    /// Encrypts the calldata, signs the request (preventing `msg.sender`
    /// spoofing), and decrypts the response.
    fn call(&self) -> impl std::future::Future<Output = TransportResult<C::Return>> + Send
    where
        C::Return: Send;

    /// Send an encrypted write transaction.
    ///
    /// The filler pipeline detects the seismic tx type and encrypts the
    /// calldata automatically before broadcasting.
    fn send(
        &self,
    ) -> impl std::future::Future<Output = TransportResult<PendingTransactionBuilder<N>>> + Send;

    /// Unnecessary — functions with shielded parameters are automatically
    /// sent as seismic (encrypted) transactions. Use `.call()` or `.send()` directly.
    #[deprecated = "unnecessary — functions with shielded parameters are automatically sent as seismic transactions. Use .call() or .send() directly."]
    fn seismic(self) -> ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>>;

    /// Set the block number after which this transaction expires.
    ///
    /// By default, the filler sets this to `current_block + BLOCKS_WINDOW` (100).
    fn expires_at(self, block: u64) -> Self;

    /// Set the recent block hash for chain-state pinning.
    ///
    /// By default, the filler fetches the latest block hash.
    fn recent_block_hash(self, hash: B256) -> Self;

    /// Set a custom encryption nonce (AEAD nonce).
    ///
    /// By default, the filler generates a random nonce. Only override this
    /// for deterministic testing — reusing nonces in production breaks encryption.
    fn encryption_nonce(self, nonce: U96) -> Self;

    /// Use EIP-712 typed data signing instead of standard RLP signing.
    ///
    /// Primarily needed for browser wallet (e.g., MetaMask) integration.
    fn eip712(self) -> Self;
}

impl<'a, P, C, N> ShieldedCallExt<'a, P, C, N> for ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>>
where
    N: SeismicNetwork,
    C: SolCall + Send + Sync,
    P: SignedProviderExt<N>,
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    async fn call(&self) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        let request = build_seismic_request(&self.0);
        let result = self.0.provider.seismic_call(SendableTx::Builder(request)).await?;
        C::abi_decode_returns(&result)
            .map_err(|e| SeismicProviderError::AbiDecode(e).into_transport())
    }

    async fn send(&self) -> TransportResult<PendingTransactionBuilder<N>> {
        let request = build_seismic_request(&self.0);

        let seismic_req: &SeismicTransactionRequest = request.as_ref();
        let is_eip712 =
            seismic_req.seismic_elements.as_ref().map_or(false, |e| e.message_version >= 2);

        if is_eip712 {
            self.0.provider.eip712_send(SendableTx::Builder(request)).await
        } else {
            self.0.provider.send_transaction(request).await
        }
    }

    fn seismic(self) -> ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>> {
        self
    }

    fn expires_at(self, block: u64) -> Self {
        mutate_shielded_elements(self, |e| e.expires_at_block = block)
    }

    fn recent_block_hash(self, hash: B256) -> Self {
        mutate_shielded_elements(self, |e| e.recent_block_hash = hash)
    }

    fn encryption_nonce(self, nonce: U96) -> Self {
        mutate_shielded_elements(self, |e| e.encryption_nonce = nonce)
    }

    fn eip712(self) -> Self {
        mutate_shielded_elements(self, |e| e.message_version = 2)
    }
}

// ============================================================================
// Private helpers
// ============================================================================

/// Build a seismic request from a `SolCallBuilder`'s underlying request.
fn build_seismic_request<N: SeismicNetwork>(
    builder: &SolCallBuilder<&impl Provider<N>, impl SolCall, N>,
) -> N::TransactionRequest
where
    N::TransactionRequest: AsMut<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    let mut request = builder.as_ref().clone();
    let seismic_req: &mut SeismicTransactionRequest = request.as_mut();
    seismic_req.inner.transaction_type = Some(seismic_alloy_consensus::TxSeismic::TX_TYPE);
    request
}

/// Mutate seismic elements on a `ShieldedCallBuilder`'s underlying request.
fn mutate_shielded_elements<'a, P: Provider<N>, C: SolCall, N: SeismicNetwork>(
    mut builder: ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>>,
    f: impl FnOnce(&mut TxSeismicElements),
) -> ShieldedCallBuilder<SolCallBuilder<&'a P, C, N>>
where
    N::TransactionRequest: AsMut<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    builder.0 = builder.0.map(|mut req: N::TransactionRequest| {
        let seismic_req: &mut SeismicTransactionRequest = req.as_mut();
        let elements = seismic_req.seismic_elements.get_or_insert_with(TxSeismicElements::default);
        f(elements);
        req
    });
    builder
}
