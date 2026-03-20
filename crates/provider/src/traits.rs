//! Seismic provider traits.
//!
//! Two traits extend alloy's `Provider`:
//!
//! - [`SeismicProviderExt`] — available on **all** Seismic providers (signed and unsigned).
//!   Provides `transparent_call`, `transparent_send`, and `get_tee_pubkey`.
//!
//! - [`SignedProviderExt`] — available only on **signed** providers. Provides `shielded_call`,
//!   `shielded_send`, `seismic_call`, and `eip712_send`. This trait is sealed — only
//!   [`ResponseDecryptProvider`](crate::decrypt::ResponseDecryptProvider) implements it.
use alloy_network::TransactionBuilder;
use alloy_primitives::Address;
use alloy_provider::{PendingTransactionBuilder, Provider, RootProvider};
use alloy_sol_types::SolCall;
use alloy_transport::TransportResult;

use alloy_provider::fillers::{FillProvider, TxFiller};

use crate::SeismicProviderError;
use seismic_alloy_network::{
    foundry::SeismicFoundry, seismic_network::SeismicNetwork, SeismicReth,
};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use seismic_enclave::secp256k1::PublicKey;

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
