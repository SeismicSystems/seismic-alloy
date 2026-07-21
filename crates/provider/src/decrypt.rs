//! Response decryption layer for Seismic signed providers.
//!
//! This module provides [`ResponseDecryptProvider`], a single wrapper that holds
//! the crypto keys needed for response decryption. It sits on top of a
//! [`FillProvider`] and is the only layer between user code and alloy's provider
//! stack.
use alloy_network::TransactionBuilder;
use alloy_primitives::Bytes;
use alloy_provider::{
    fillers::{FillProvider, TxFiller},
    PendingTransactionBuilder, Provider, ProviderLayer, RootProvider, SendableTx,
};
use alloy_transport::TransportResult;
use seismic_alloy_consensus::InputDecryptionElements;

use crate::SeismicProviderError;
use seismic_alloy_network::seismic_network::SeismicNetwork;
use seismic_alloy_rpc_types::SeismicTransactionRequest;

use crate::{SeismicProviderExt, SignedProviderExt};

/// Provider wrapper that adds response decryption for shielded reads.
///
/// This is the only Seismic-specific provider wrapper. It holds:
/// - A provider secret key (for ECDH-based decryption)
/// - The TEE public key (fetched once at provider creation)
///
/// For unsigned providers (no decryption needed), use a bare `FillProvider` —
/// no wrapper required.
#[derive(Debug, Clone)]
pub struct ResponseDecryptProvider<N, P> {
    inner: P,
    provider_secret_key: seismic_crypto::secp256k1::SecretKey,
    tee_pubkey: seismic_crypto::secp256k1::PublicKey,
    _network: std::marker::PhantomData<N>,
}

impl<N, P> ResponseDecryptProvider<N, P> {
    /// Create a new response-decrypting provider.
    pub fn new(
        inner: P,
        provider_secret_key: seismic_crypto::secp256k1::SecretKey,
        tee_pubkey: seismic_crypto::secp256k1::PublicKey,
    ) -> Self {
        Self { inner, provider_secret_key, tee_pubkey, _network: std::marker::PhantomData }
    }
}

// ---------------------------------------------------------------------------
// Provider impl — delegates everything to inner
// ---------------------------------------------------------------------------

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N: SeismicNetwork, P> Provider<N> for ResponseDecryptProvider<N, P>
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    P: Provider<N>,
{
    fn root(&self) -> &RootProvider<N> {
        self.inner.root()
    }

    async fn send_transaction_internal(
        &self,
        tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        self.inner.send_transaction_internal(tx).await
    }
}

// ---------------------------------------------------------------------------
// SeismicProviderExt impl — inherits defaults (transparent_call, etc.)
// ---------------------------------------------------------------------------

impl<N, F, P> SeismicProviderExt<N> for ResponseDecryptProvider<N, FillProvider<F, P, N>>
where
    N: SeismicNetwork,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    N::UnsignedTx: Send + Sync,
    F: TxFiller<N>,
    P: Provider<N>,
{
}

// ---------------------------------------------------------------------------
// SignedProviderExt impl — fills, sends, and decrypts
// ---------------------------------------------------------------------------

/// Helper: decrypt a response using seismic metadata and crypto keys.
fn decrypt_response(
    output: &Bytes,
    metadata: &seismic_alloy_consensus::TxSeismicMetadata,
    tee_pubkey: &seismic_crypto::secp256k1::PublicKey,
    provider_secret_key: &seismic_crypto::secp256k1::SecretKey,
) -> TransportResult<Bytes> {
    metadata
        .seismic_elements
        .client_decrypt(output, tee_pubkey, provider_secret_key, metadata)
        .map_err(|e| SeismicProviderError::Decryption(format!("{e:?}")).into_transport())
}

/// Helper: send a filled seismic tx as an eth_call RPC (no decryption).
async fn raw_seismic_call<N: SeismicNetwork>(
    root: &alloy_provider::RootProvider<N>,
    tx: SendableTx<N>,
) -> TransportResult<Bytes>
where
    N::UnsignedTx: Send + Sync,
{
    use alloy_network::eip2718::Encodable2718;
    match tx {
        SendableTx::Builder(builder) => {
            let output: Bytes = root.client().request("eth_call", (builder,)).await?;
            Ok(output)
        }
        SendableTx::Envelope(envelope) => {
            if let Some(typed_data_req) = N::to_typed_data_request(&envelope) {
                let output: Bytes = root.client().request("eth_call", (typed_data_req,)).await?;
                Ok(output)
            } else {
                let encoded_tx = envelope.encoded_2718();
                let output = root.client().request("eth_call", (encoded_tx,)).await?;
                Ok(output)
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<N, F, P> SignedProviderExt<N> for ResponseDecryptProvider<N, FillProvider<F, P, N>>
where
    N: SeismicNetwork,
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + From<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
    F: TxFiller<N>,
    P: Provider<N>,
{
    async fn seismic_call_raw(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        match tx {
            SendableTx::Builder(builder) => {
                // Check if this is a seismic transaction
                let seismic_req: &SeismicTransactionRequest = builder.as_ref();
                if !seismic_req.is_seismic() {
                    // Not seismic, just pass through as a standard eth_call
                    return self.call(builder).await;
                }

                // Mark as signed_read before filling
                let modified_req = seismic_req.clone().with_signed_read();
                let modified_builder: N::TransactionRequest = modified_req.into();

                // Fill the transaction (wallet, nonce, chain_id, seismic elements, gas)
                let filled_tx = self.inner.fill(modified_builder).await?;

                // Extract metadata for decryption
                let metadata = match &filled_tx {
                    SendableTx::Builder(filled_builder) => {
                        let sender = filled_builder
                            .from()
                            .ok_or_else(|| SeismicProviderError::MissingSender.into_transport())?;
                        filled_builder.metadata(sender).map_err(|e| {
                            SeismicProviderError::MetadataCreation(format!("{e:?}"))
                                .into_transport()
                        })?
                    }
                    SendableTx::Envelope(envelope) => N::extract_seismic_metadata(envelope)
                        .ok_or_else(|| SeismicProviderError::NotSeismicEnvelope.into_transport())?,
                };

                // Send the RPC call (without decryption)
                let output = raw_seismic_call(self.inner.root(), filled_tx).await?;

                // Decrypt the response
                decrypt_response(&output, &metadata, &self.tee_pubkey, &self.provider_secret_key)
            }
            SendableTx::Envelope(envelope) => {
                // Envelope passed directly — try to extract seismic metadata
                if let Some(metadata) = N::extract_seismic_metadata(&envelope) {
                    let output =
                        raw_seismic_call(self.inner.root(), SendableTx::Envelope(envelope)).await?;
                    decrypt_response(
                        &output,
                        &metadata,
                        &self.tee_pubkey,
                        &self.provider_secret_key,
                    )
                } else {
                    // Not a seismic envelope, pass through
                    raw_seismic_call(self.inner.root(), SendableTx::Envelope(envelope)).await
                }
            }
        }
    }

    async fn eip712_send(
        &self,
        tx: SendableTx<N>,
    ) -> TransportResult<PendingTransactionBuilder<N>> {
        // Fill the transaction (wallet, nonce, chain_id, seismic elements, gas, sign)
        let filled = match tx {
            SendableTx::Builder(builder) => self.inner.fill(builder).await?,
            envelope => envelope,
        };

        // Extract TypedDataRequest from the signed envelope
        match filled {
            SendableTx::Envelope(ref envelope) => {
                let typed_data_req = N::to_typed_data_request(envelope).ok_or_else(|| {
                    SeismicProviderError::Eip712NotSeismicEnvelope.into_transport()
                })?;

                let tx_hash =
                    self.client().request("eth_sendRawTransaction", (typed_data_req,)).await?;
                Ok(PendingTransactionBuilder::new(self.root().clone(), tx_hash))
            }
            SendableTx::Builder(_) => Err(SeismicProviderError::Eip712GotBuilder.into_transport()),
        }
    }
}

// ---------------------------------------------------------------------------
// ProviderLayer impl — for use with ProviderBuilder
// ---------------------------------------------------------------------------

/// Layer that wraps a provider with [`ResponseDecryptProvider`].
#[derive(Debug, Clone)]
pub struct ResponseDecryptLayer {
    provider_secret_key: seismic_crypto::secp256k1::SecretKey,
    tee_pubkey: seismic_crypto::secp256k1::PublicKey,
}

impl ResponseDecryptLayer {
    /// Create a new response decrypt layer.
    pub fn new(
        provider_secret_key: seismic_crypto::secp256k1::SecretKey,
        tee_pubkey: seismic_crypto::secp256k1::PublicKey,
    ) -> Self {
        Self { provider_secret_key, tee_pubkey }
    }
}

impl<N: SeismicNetwork, P> ProviderLayer<P, N> for ResponseDecryptLayer
where
    N::UnsignedTx: Send + Sync,
    N::TransactionRequest: From<SeismicTransactionRequest>,
    P: Provider<N>,
{
    type Provider = ResponseDecryptProvider<N, P>;

    fn layer(&self, inner: P) -> Self::Provider {
        ResponseDecryptProvider::new(inner, self.provider_secret_key.clone(), self.tee_pubkey)
    }
}
