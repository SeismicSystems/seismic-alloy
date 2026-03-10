//! Seismic provider trait that extends [`alloy_provider::Provider`] with Seismic-specific
//! functionality including encrypted (shielded) and standard (transparent) contract interactions.
use alloy_network::{eip2718::Encodable2718, TransactionBuilder};
use alloy_primitives::{Address, Bytes};
use alloy_provider::{
    fillers::{FillProvider, TxFiller},
    PendingTransactionBuilder, Provider, RootProvider, SendableTx,
};
use alloy_sol_types::SolCall;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_network::{
    foundry::SeismicFoundry, seismic_network::SeismicNetwork, SeismicReth,
};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use seismic_enclave::secp256k1::PublicKey;

/// Extends [`alloy_provider::Provider`] with Seismic-specific functionality.
///
/// Provides low-level [`seismic_call`](SeismicProviderExt::seismic_call) and high-level
/// ergonomic methods that integrate with alloy's `sol!` macro:
///
/// - [`shielded_call`](SeismicProviderExt::shielded_call) — encrypted signed read
/// - [`shielded_send`](SeismicProviderExt::shielded_send) — encrypted write transaction
/// - [`transparent_call`](SeismicProviderExt::transparent_call) — standard `eth_call`
/// - [`transparent_send`](SeismicProviderExt::transparent_send) — standard transaction
///
/// # Example
///
/// ```rust,ignore
/// use alloy_sol_types::sol;
///
/// sol! {
///     interface MyContract {
///         function getValue() public view returns (uint256);
///         function setValue(uint256 newValue) public;
///     }
/// }
///
/// // Encrypted read with response decryption (requires signed provider)
/// let result = provider
///     .shielded_call(contract_addr, MyContract::getValueCall {})
///     .await?;
///
/// // Encrypted write transaction
/// let receipt = provider
///     .shielded_send(contract_addr, MyContract::setValueCall { newValue: U256::from(42) })
///     .await?
///     .get_receipt()
///     .await?;
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SeismicProviderExt<N: SeismicNetwork>: Provider<N>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: From<SeismicTransactionRequest>,
{
    // ========================================================================
    // High-level contract interaction methods (sol! macro integration)
    // ========================================================================

    /// Encrypted, signed read call. Encrypts calldata, signs the call
    /// (preventing `msg.sender` spoofing), and decrypts the response.
    /// Requires a **signed provider**.
    async fn shielded_call<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<C::Return>
    where
        C::Return: Send,
    {
        let encoded = call.abi_encode();
        let tx: N::TransactionRequest =
            SeismicTransactionRequest::default().to(address).seismic().into();
        let mut tx = tx;
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        let result = self.seismic_call(SendableTx::Builder(tx)).await?;

        C::abi_decode_returns(&result)
            .map_err(|e| TransportErrorKind::custom_str(&format!("ABI decode error: {e}")))
    }

    /// Encrypted write transaction. The filler pipeline handles encryption key
    /// generation, nonce, gas estimation, and signing.
    async fn shielded_send<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        let encoded = call.abi_encode();
        let tx: N::TransactionRequest =
            SeismicTransactionRequest::default().to(address).seismic().into();
        let mut tx = tx;
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        self.send_transaction(tx).await
    }

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
        let tx: N::TransactionRequest = SeismicTransactionRequest::default().to(address).into();
        let mut tx = tx;
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        let result = self.call(tx).await?;

        C::abi_decode_returns(&result)
            .map_err(|e| TransportErrorKind::custom_str(&format!("ABI decode error: {e}")))
    }

    /// Standard (unencrypted) write transaction.
    async fn transparent_send<C: SolCall + Send>(
        &self,
        address: Address,
        call: C,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        let encoded = call.abi_encode();
        let tx: N::TransactionRequest = SeismicTransactionRequest::default().to(address).into();
        let mut tx = tx;
        TransactionBuilder::<N>::set_input(&mut tx, encoded);

        self.send_transaction(tx).await
    }

    // ========================================================================
    // Low-level methods
    // ========================================================================

    /// Makes a call request while handling seismic specific aspects
    /// e.g. sending signed call requests
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        self.call_conditionally_signed(tx).await
    }

    /// Whether the input data should be encrypted.
    /// If it's not a seismic tx, don't encrypt.
    /// If it is, only encrypt if it's non-empty.
    fn should_encrypt_input<B: TransactionBuilder<N>>(&self, tx: &B) -> bool {
        if !N::is_seismic_tx_type(tx.output_tx_type()) {
            return false;
        }
        tx.input().map_or(false, |input| !input.is_empty())
    }

    /// Get the PublicKey of the enclave
    async fn get_tee_pubkey(&self) -> TransportResult<PublicKey> {
        seismic_alloy_network::fetch_tee_pubkey(self).await
    }

    /// Makes a call request, perhaps making the call signed depending on the input type
    async fn call_conditionally_signed(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        match tx {
            SendableTx::Builder(builder) => {
                let output: Bytes = self.client().request("eth_call", (builder.clone(),)).await?;
                Ok(output)
            }
            SendableTx::Envelope(envelope) => {
                let encoded_tx = envelope.encoded_2718();
                let output = self.client().request("eth_call", (encoded_tx,)).await?;
                Ok(output)
            }
        }
    }
}

impl SeismicProviderExt<SeismicReth> for RootProvider<SeismicReth> {}
impl SeismicProviderExt<SeismicFoundry> for RootProvider<SeismicFoundry> {}

/// Blanket impl so `&T: SeismicProviderExt` when `T: SeismicProviderExt`.
/// This mirrors alloy's blanket `Provider` impl for `&T` and is needed so that
/// `#[sol(rpc)]`-generated contract types (which store `&provider`) work with
/// `.seismic()`.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<T, N> SeismicProviderExt<N> for &T
where
    T: SeismicProviderExt<N> + Sync,
    N: SeismicNetwork,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
{
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        (**self).seismic_call(tx).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<F, P, N> SeismicProviderExt<N> for FillProvider<F, P, N>
where
    N: SeismicNetwork,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    F: TxFiller<N>,
    P: Provider<N>,
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        // Fill the transaction
        let builder = tx.as_builder().unwrap().clone();

        let built_tx = self.fill(builder).await?;

        // self.inner is not public for FillProvider.
        // However, for our use cases, self.inner is the RootProvider,
        // so we get it this hacky way
        let inner = self.root();
        SeismicProviderExt::seismic_call(inner, built_tx).await
    }
}
