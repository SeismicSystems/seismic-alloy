//! Seismic provider traits.
//!
//! Two traits extend alloy's `Provider`:
//!
//! - [`SeismicProviderExt`] — available on **all** Seismic providers (signed and unsigned).
//!   Provides `transparent_call`, `transparent_send`, and `get_tee_pubkey`.
//!
//! - [`SignedProviderExt`] — available only on **signed** providers. Provides `seismic_call`,
//!   `seismic_send`, `seismic_call_raw`, and `eip712_send`, plus `_with` variants that accept
//!   [`SecurityParams`]. This trait is sealed — only
//!   [`ResponseDecryptProvider`](crate::decrypt::ResponseDecryptProvider) implements it.
use alloy_network::TransactionBuilder;
use alloy_primitives::{Address, Bytes};
use alloy_provider::{PendingTransactionBuilder, Provider, RootProvider, SendableTx};
use alloy_sol_types::SolCall;
use alloy_transport::TransportResult;
use seismic_alloy_consensus::TxSeismicElements;

use alloy_provider::fillers::{FillProvider, TxFiller};

use crate::{SecurityParams, SeismicProviderError};
use seismic_alloy_network::{
    foundry::SeismicFoundry, seismic_network::SeismicNetwork, SeismicReth,
};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use seismic_crypto::secp256k1::PublicKey;

// ============================================================================
// SeismicProviderExt — base trait for all Seismic providers
// ============================================================================

/// Base extension trait for all Seismic providers (signed and unsigned).
///
/// Provides standard (unencrypted) contract interaction methods and TEE key access.
/// For shielded (encrypted) operations, use a signed provider which also implements
/// [`SignedProviderExt`].
///
/// # Example
///
/// ```rust,ignore
/// use seismic_alloy_provider::SeismicProviderExt;
///
/// // Works on any provider (signed or unsigned)
/// let result: bool = provider
///     .transparent_call(contract_addr, MyContract::isOddCall {})
///     .await?;
///
/// let tee_pubkey = provider.get_tee_pubkey().await?;
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SeismicProviderExt<N: SeismicNetwork>: Provider<N>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: From<SeismicTransactionRequest>,
{
    /// Standard (unencrypted) `eth_call`. Use for public view functions.
    async fn transparent_call<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        let encoded = call.abi_encode();
        let mut tx: N::TransactionRequest = SeismicTransactionRequest::default().to(address).into();
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        let result = self.call(tx).await?;

        C::abi_decode_returns(&result)
            .map_err(|e| SeismicProviderError::AbiDecode(e).into_transport())
    }

    /// Standard (unencrypted) write transaction.
    async fn transparent_send<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        let encoded = call.abi_encode();
        let mut tx: N::TransactionRequest = SeismicTransactionRequest::default().to(address).into();
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        self.send_transaction(tx).await
    }

    /// Get the PublicKey of the enclave.
    async fn get_tee_pubkey(&self) -> TransportResult<PublicKey> {
        seismic_alloy_network::fetch_tee_pubkey(self).await
    }
}

// ---------------------------------------------------------------------------
// Blanket impls for SeismicProviderExt
// ---------------------------------------------------------------------------

impl SeismicProviderExt<SeismicReth> for RootProvider<SeismicReth> {}
impl SeismicProviderExt<SeismicFoundry> for RootProvider<SeismicFoundry> {}

/// Blanket impl so `&T: SeismicProviderExt` when `T: SeismicProviderExt`.
/// Needed for `#[sol(rpc)]`-generated contract types which store `&provider`.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<T, N> SeismicProviderExt<N> for &T
where
    T: SeismicProviderExt<N> + Sync,
    N: SeismicNetwork,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
}

/// Blanket impl for `FillProvider` (used by unsigned providers).
impl<F, P, N> SeismicProviderExt<N> for FillProvider<F, P, N>
where
    N: SeismicNetwork,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    F: TxFiller<N>,
    P: Provider<N>,
    N::UnsignedTx: Send + Sync,
{
}

// ============================================================================
// SignedProviderExt — sealed trait for signed providers
// ============================================================================

/// Sealed trait for signed providers that can encrypt calldata and decrypt responses.
///
/// Only implemented by [`ResponseDecryptProvider`](crate::decrypt::ResponseDecryptProvider)
/// and references to it. Provides:
/// - Low-level: `seismic_call_raw`, `eip712_send`
/// - High-level: `seismic_call`, `seismic_send` (with `_with` variants for [`SecurityParams`])
///
/// Users typically don't interact with this trait directly — use the
/// call-builder traits ([`SeismicCallExt`](crate::SeismicCallExt),
/// [`ShieldedCallExt`](crate::ShieldedCallExt)) instead.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SignedProviderExt<N: SeismicNetwork>: SeismicProviderExt<N>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: From<SeismicTransactionRequest>,
{
    /// Low-level seismic call. Fills the transaction, sends as `eth_call`,
    /// and decrypts the response.
    async fn seismic_call_raw(&self, tx: SendableTx<N>) -> TransportResult<Bytes>;

    /// Send an EIP-712 signed seismic transaction.
    async fn eip712_send(&self, tx: SendableTx<N>)
        -> TransportResult<PendingTransactionBuilder<N>>;

    /// Encrypted, signed read call. Encrypts calldata, signs the call
    /// (preventing `msg.sender` spoofing), and decrypts the response.
    async fn seismic_call<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        self.seismic_call_with(address, call, SecurityParams::default()).await
    }

    /// Encrypted write transaction. The filler pipeline handles encryption key
    /// generation, nonce, gas estimation, and signing.
    async fn seismic_send<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        self.seismic_send_with(address, call, SecurityParams::default()).await
    }

    /// Encrypted, signed read call with custom security parameters.
    async fn seismic_call_with<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
        params: SecurityParams,
    ) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        let encoded = call.abi_encode();
        let mut seismic_req = SeismicTransactionRequest::default().to(address).seismic();

        // Apply security params to the partial elements
        let elements = seismic_req.seismic_elements.get_or_insert_with(TxSeismicElements::default);
        params.apply_to(elements);

        let mut tx: N::TransactionRequest = seismic_req.into();
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        // For reads, EIP-712 affects signing (handled by the filler) but the
        // call path is the same — always goes through seismic_call_raw.
        let result = self.seismic_call_raw(SendableTx::Builder(tx)).await?;
        C::abi_decode_returns(&result)
            .map_err(|e| SeismicProviderError::AbiDecode(e).into_transport())
    }

    /// Encrypted write transaction with custom security parameters.
    async fn seismic_send_with<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
        params: SecurityParams,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        let encoded = call.abi_encode();
        let mut seismic_req = SeismicTransactionRequest::default().to(address).seismic();

        // Apply encryption params to the partial elements
        let elements = seismic_req.seismic_elements.get_or_insert_with(TxSeismicElements::default);
        params.apply_to(elements);

        let mut tx: N::TransactionRequest = seismic_req.into();
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
    async fn seismic_call_raw(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        (**self).seismic_call_raw(tx).await
    }

    async fn eip712_send(
        &self,
        tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        (**self).eip712_send(tx).await
    }
}
