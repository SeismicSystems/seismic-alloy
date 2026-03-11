//! Extension trait for alloy's [`CallBuilder`] that adds `.seismic()` support.
//!
//! This enables the standard `#[sol(rpc)]` pattern to work with Seismic encryption:
//!
//! ```rust,ignore
//! use seismic_alloy_provider::SeismicCallExt;
//!
//! sol! {
//!     #[sol(rpc)]
//!     interface MyContract {
//!         function isOdd() public view returns (bool);
//!         function setNumber(uint256 newNumber) public;
//!     }
//! }
//!
//! let contract = MyContract::new(address, &provider);
//!
//! // Shielded read (encrypted + signed)
//! let is_odd = contract.isOdd().seismic().call().await?;
//!
//! // Shielded send (encrypted write)
//! let receipt = contract.setNumber(U256::from(42)).seismic().send().await?
//!     .get_receipt().await?;
//!
//! // Transparent (default alloy behavior, no changes needed)
//! let is_odd = contract.isOdd().call().await?;
//! ```
use alloy_contract::SolCallBuilder;
use alloy_network::Network;
use alloy_primitives::{aliases::U96, B256};
use alloy_provider::{PendingTransactionBuilder, SendableTx};
use alloy_sol_types::SolCall;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::TxSeismicElements;
use seismic_alloy_network::seismic_network::SeismicNetwork;
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use std::marker::PhantomData;

use crate::SeismicProviderExt;

/// Extension trait that adds `.seismic()` to alloy's [`SolCallBuilder`].
///
/// Calling `.seismic()` marks the call/transaction as encrypted. The returned
/// [`SeismicSolCallBuilder`] provides:
/// - `.call()` — encrypted, signed read (routes through the filler pipeline)
/// - `.send()` — encrypted write (fillers handle encryption automatically)
pub trait SeismicCallExt<'a, P, C: SolCall, N: Network> {
    /// Mark this contract call as a seismic (encrypted) operation.
    ///
    /// For reads (`.call()`), this encrypts the calldata, signs the request
    /// (preventing `msg.sender` spoofing), and decrypts the response.
    ///
    /// For writes (`.send()`), this sets the seismic tx type so the filler
    /// pipeline encrypts the calldata before submission.
    fn seismic(self) -> SeismicSolCallBuilder<'a, P, C, N>;
}

impl<'a, P, C, N> SeismicCallExt<'a, P, C, N> for SolCallBuilder<&'a P, C, N>
where
    N: SeismicNetwork,
    C: SolCall,
    P: SeismicProviderExt<N>,
    N::TransactionRequest: AsMut<SeismicTransactionRequest> + From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    fn seismic(self) -> SeismicSolCallBuilder<'a, P, C, N> {
        // Set the seismic tx type on the underlying request via .map()
        let inner = self.map(|mut req| {
            let seismic_req: &mut SeismicTransactionRequest = req.as_mut();
            seismic_req.inner.transaction_type = Some(seismic_alloy_consensus::TxSeismic::TX_TYPE);
            req
        });
        SeismicSolCallBuilder { inner, _call: PhantomData }
    }
}

/// A [`SolCallBuilder`] wrapper that routes calls through Seismic's encrypted path.
///
/// Created by calling [`.seismic()`](SeismicCallExt::seismic) on a `SolCallBuilder`.
///
/// - `.call()` goes through [`SeismicProviderExt::seismic_call`] which runs the filler
///   pipeline (encrypting calldata, signing the request, then decrypting the response).
/// - `.send()` delegates to the standard `send_transaction` path — the fillers detect
///   the seismic tx type and encrypt automatically.
#[must_use = "call builders do nothing unless you `.call()` or `.send()` them"]
pub struct SeismicSolCallBuilder<'a, P, C: SolCall, N: Network> {
    inner: SolCallBuilder<&'a P, C, N>,
    _call: PhantomData<C>,
}

impl<'a, P, C, N> SeismicSolCallBuilder<'a, P, C, N>
where
    N: SeismicNetwork,
    C: SolCall,
    P: SeismicProviderExt<N>,
    N::TransactionRequest: AsMut<SeismicTransactionRequest> + From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    /// Set the block number after which this transaction expires.
    ///
    /// By default, the filler sets this to `current_block + BLOCKS_WINDOW` (100).
    /// Use this to shorten or extend the validity window.
    pub fn expires_at(self, block: u64) -> Self {
        self.mutate_elements(|e| e.expires_at_block = block)
    }

    /// Set the recent block hash for chain-state pinning.
    ///
    /// By default, the filler fetches the latest block hash. Use this to pin
    /// the transaction to a specific chain state (e.g., for deterministic testing).
    pub fn recent_block_hash(self, hash: B256) -> Self {
        self.mutate_elements(|e| e.recent_block_hash = hash)
    }

    /// Set a custom encryption nonce (AEAD nonce).
    ///
    /// By default, the filler generates a random nonce. Only override this
    /// for deterministic testing — reusing nonces in production breaks encryption.
    pub fn encryption_nonce(self, nonce: U96) -> Self {
        self.mutate_elements(|e| e.encryption_nonce = nonce)
    }

    /// Internal: mutate the partial seismic elements on the underlying request.
    fn mutate_elements(mut self, f: impl FnOnce(&mut TxSeismicElements)) -> Self {
        self.inner = self.inner.map(|mut req| {
            let seismic_req: &mut SeismicTransactionRequest = req.as_mut();
            let elements = seismic_req.seismic_elements.get_or_insert_with(TxSeismicElements::default);
            f(elements);
            req
        });
        self
    }
}

impl<'a, P, C, N> SeismicSolCallBuilder<'a, P, C, N>
where
    N: SeismicNetwork,
    C: SolCall + Send,
    P: SeismicProviderExt<N>,
    N::TransactionRequest: AsMut<SeismicTransactionRequest> + From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    /// Execute an encrypted, signed read call.
    ///
    /// The call goes through the filler pipeline which encrypts the calldata
    /// and signs the request. The response is decrypted before being returned.
    pub async fn call(&self) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        // Clone the request (already has seismic tx type set from .seismic())
        let request = self.inner.as_ref().clone();

        // Route through seismic_call which runs the filler pipeline
        // (encrypts calldata, signs, sends via eth_call, decrypts response)
        let result = self.inner.provider.seismic_call(SendableTx::Builder(request)).await?;

        C::abi_decode_returns(&result)
            .map_err(|e| TransportErrorKind::custom_str(&format!("ABI decode error: {e}")))
    }

    /// Send an encrypted write transaction.
    ///
    /// The fillers detect the seismic tx type and encrypt the calldata
    /// automatically before broadcasting.
    pub async fn send(&self) -> TransportResult<PendingTransactionBuilder<N>> {
        // Clone the request (already has seismic tx type set)
        let request = self.inner.as_ref().clone();

        // Standard send_transaction — fillers handle encryption
        self.inner.provider.send_transaction(request).await
    }
}

impl<P, C: SolCall, N: Network> std::fmt::Debug for SeismicSolCallBuilder<'_, P, C, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeismicSolCallBuilder").field("inner", &self.inner).finish()
    }
}
