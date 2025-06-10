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
use seismic_alloy_consensus::{InputDecryptionElements, SeismicTxType, TxSeismicElements};
use seismic_alloy_network::Seismic;
use seismic_enclave::PublicKey;
use std::str::FromStr;

/// Extends the alloy_provider::Provider with Seismic specific functionality
#[async_trait::async_trait]
pub trait SeismicProviderExt: Provider<Seismic> {
    /// Makes a call request while handling seismic specific aspects
    /// e.g. encrypting input data and decrypting output data
    /// e.g. sending signed call requests
    async fn seismic_call(&self, tx: SendableTx<Seismic>) -> TransportResult<Bytes> {
        self.call_conditionally_signed(tx).await
    }

    /// Whether the input data should be encrypted
    /// None or Empty input data should not be encrypted
    fn should_encrypt_input<B: TransactionBuilder<Seismic>>(&self, tx: &B) -> bool {
        if tx.output_tx_type() == SeismicTxType::Seismic {
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
    async fn call_conditionally_signed(&self, tx: SendableTx<Seismic>) -> TransportResult<Bytes> {
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

impl SeismicProviderExt for RootProvider<Seismic> {}

#[async_trait::async_trait]
impl<F, P> SeismicProviderExt for FillProvider<F, P, Seismic>
where
    F: TxFiller<Seismic>,
    P: Provider<Seismic>,
{
    async fn seismic_call(&self, tx: SendableTx<Seismic>) -> TransportResult<Bytes> {
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
