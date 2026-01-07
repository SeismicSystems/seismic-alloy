//! A seismic provider trait that extends alloy_provider::Provider and is implimented for relevant
//! types. Extends the provider trait with ...
use alloy_network::{eip2718::Encodable2718, TransactionBuilder};
use alloy_primitives::Bytes;
use alloy_provider::{
    fillers::{FillProvider, TxFiller},
    Provider, ProviderCall, RootProvider, SendableTx,
};
use alloy_rpc_client::NoParams;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_network::{
    foundry::SeismicFoundry, seismic_network::SeismicNetwork, SeismicReth,
};
use seismic_enclave::secp256k1::PublicKey;
use std::str::FromStr;

/// Extends the alloy_provider::Provider with Seismic specific functionality
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SeismicProviderExt<N: SeismicNetwork>: Provider<N>
where
    N::UnsignedTx: Send + Sync,
{
    /// Makes a call request while handling seismic specific aspects
    /// e.g. sending signed call requests
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        self.call_conditionally_signed(tx).await
    }

    /// Whether the input data should be encrypted
    /// If it's not a seismic tx, don't encrypt.
    /// If it is, only encrypt if it's non-empty
    fn should_encrypt_input<B: TransactionBuilder<N>>(&self, tx: &B) -> bool {
        if !N::is_seismic_tx_type(tx.output_tx_type()) {
            return false;
        }
        tx.input().map_or(false, |input| !input.is_empty())
    }

    /// Get the PublicKey of the enclave
    async fn get_tee_pubkey(&self) -> TransportResult<PublicKey> {
        let call: ProviderCall<NoParams, String> =
            self.client().request_noparams("seismic_getTeePublicKey").into();
        let resp = call.await?;
        let stripped = resp.strip_prefix("0x").unwrap_or(&resp);
        match PublicKey::from_str(stripped) {
            Ok(pk) => Ok(pk),
            Err(e) => Err(TransportErrorKind::custom_str(&format!(
                "Error getting tee pubkey from server: {:?}",
                e
            ))),
        }
    }

    /// Makes a call request, perhaps making the call signed depinding on the input type
    async fn call_conditionally_signed(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        match tx {
            SendableTx::Builder(builder) => {
                let output = self.client().request("eth_call", (builder.clone(),)).await?;
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

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<F, P, N> SeismicProviderExt<N> for FillProvider<F, P, N>
where
    N: SeismicNetwork,
    F: TxFiller<N>,
    P: Provider<N> + SeismicProviderExt<N>,
    N::UnsignedTx: Send + Sync,
{
    // No custom seismic_call implementation - use the default from the trait
    // which calls call_conditionally_signed. This allows the call to properly
    // flow through provider layers (fill happens via alloy's built-in mechanisms)
}
