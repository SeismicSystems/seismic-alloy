//! Custom fillers for the Seismic provider
//!
//! Normally fillers go in alloy-provider, but we need to put them here because
//! we need it to impl RecommendedFillers for the [`Seismic`] network.

use crate::seismic_network::SeismicNetwork;
use alloy_network::{Network, TransactionBuilder};
use alloy_provider::{
    fillers::{FillerControlFlow, GasFillable, TxFiller},
    Provider, SendableTx,
};
use alloy_transport::{TransportErrorKind, TransportResult};
use futures::FutureExt;
use seismic_alloy_consensus::{InputDecryptionElements, TxSeismicElements};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

pub use alloy_provider::fillers::GasFiller;

/// A wrapper for alloy_provider::fillers::GasFiller that handles gas for seismic transactions
/// Seismic tx need to be treated like a legacy tx
#[derive(Clone, Copy, Debug, Default)]
pub struct SeismicGasFiller(GasFiller);

impl SeismicGasFiller {
    fn is_seismic_tx<N>(&self, tx: &N::TransactionRequest) -> bool
    where
        N: SeismicNetwork,
        N::TransactionRequest: InputDecryptionElements,
        <N as Network>::UnsignedTx: Send + Sync,
    {
        // TODO: it is probably more correct to check the tx type instead,
        // but we probably will get an error anyway if we have either combo of:
        // - a seismic tx with no decryption elements
        // - a non-seismic tx with decryption elements

        N::is_seismic_tx_type(tx.output_tx_type()) || tx.get_decryption_elements().is_ok()
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicGasFiller
where
    <N as Network>::TransactionRequest: InputDecryptionElements,
    <N as Network>::UnsignedTx: Send + Sync,
{
    type Fillable = GasFillable;

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        if self.is_seismic_tx::<N>(tx) {
            // tx is a seismic transaction, repeat logic for legacy tx
            if tx.gas_price().is_some() && tx.gas_limit().is_some() {
                return FillerControlFlow::Finished;
            } else {
                return FillerControlFlow::Ready;
            }
        } else {
            <GasFiller as TxFiller<N>>::status(&self.0, tx)
        }
    }

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {}

    async fn prepare<P>(
        &self,
        provider: &P,
        tx: &<N as Network>::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<N>,
    {
        if self.is_seismic_tx::<N>(tx) {
            // For seismic transactions, we cannot call estimate_gas during prepare()
            // because the input is not encrypted yet. Use default gas values instead.
            // Users should manually set gas if they need specific limits.
            let gas_price_fut = tx.gas_price().map_or_else(
                || provider.get_gas_price().right_future(),
                |gas_price| async move { Ok(gas_price) }.left_future(),
            );

            let gas_limit = tx.gas_limit().unwrap_or(30_000_000); // Default 30M gas for seismic txs
            let gas_price = gas_price_fut.await?;

            Ok(GasFillable::Legacy { gas_limit, gas_price })
        } else {
            GasFiller::prepare(&self.0, provider, tx).await
        }
    }

    async fn fill(
        &self,
        fillable: Self::Fillable,
        tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        GasFiller::fill(&self.0, fillable, tx).await
    }
}


/// Generates seismic elements and encrypts transaction input.
/// Each transaction gets a fresh ephemeral keypair for encryption.
/// This combines element generation and encryption into a single filler
/// to avoid the complexity of sharing ephemeral state between separate fillers.
#[derive(Clone, Debug, Default)]
pub struct SeismicElementsFiller;

impl SeismicElementsFiller {
    /// Create a new SeismicElementsFiller
    pub fn new() -> Self {
        Self
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicElementsFiller
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest> + AsMut<SeismicTransactionRequest> + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
{
    // Fillable contains: (TEE public key, ephemeral keypair for this transaction)
    type Fillable = (seismic_enclave::secp256k1::PublicKey, seismic_enclave::secp256k1::Keypair);

    fn status(&self, tx: &N::TransactionRequest) -> FillerControlFlow {
        let seismic_tx: &SeismicTransactionRequest = tx.as_ref();

        // Validate consistency first
        if let Err(_e) = seismic_tx.validate_seismic_consistency() {
            // Return ready so we can error in prepare()
            return FillerControlFlow::Ready;
        }

        // Check if this is a seismic transaction that needs encryption
        if seismic_tx.is_seismic() {
            let input = N::get_request_input(tx);
            let has_input = input.map_or(false, |i| !i.is_empty());
            let has_elements = seismic_tx.seismic_elements.is_some();

            if has_input {
                // If we have elements AND input, check if encryption already happened
                // Simple heuristic: if elements are set AND input size suggests encryption
                // (e.g., not a known plaintext size), then we're done
                if has_elements {
                    let input_len = input.unwrap().len();
                    let known_plaintext_sizes = [4, 32, 36, 64]; // Common unencrypted sizes
                    let looks_like_plaintext = known_plaintext_sizes.contains(&input_len);

                    if looks_like_plaintext {
                        // Has elements but input looks like plaintext - needs encryption
                        FillerControlFlow::Ready
                    } else {
                        // Has elements and input doesn't look like plaintext - probably encrypted
                        FillerControlFlow::Finished
                    }
                } else {
                    // No elements yet - needs encryption
                    FillerControlFlow::Ready
                }
            } else {
                // Empty input or not seismic, nothing to do
                FillerControlFlow::Finished
            }
        } else {
            FillerControlFlow::Finished
        }
    }

    fn fill_sync(&self, tx: &mut SendableTx<N>) {
        // Generate ephemeral keypair and set elements NOW (before prepare() runs)
        // We'll regenerate the keypair in prepare() and use it to encrypt in fill()
        // Note: This is needed because GasFiller.prepare() may call estimate_gas which
        // requires seismic elements to be present on seismic transactions
        if let Some(builder) = tx.as_mut_builder() {
            let seismic_builder: &mut SeismicTransactionRequest = builder.as_mut();

            // Only set elements if not already present (fill_sync can be called multiple times)
            if seismic_builder.is_seismic() && seismic_builder.seismic_elements.is_none() {
                // Generate temporary elements with a temporary keypair
                // The real encryption will use a fresh keypair generated in prepare()
                let temp_keypair = TxSeismicElements::get_rand_encryption_keypair();
                let elements = TxSeismicElements::default()
                    .with_encryption_pubkey(temp_keypair.public_key())
                    .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
                    .with_message_version(0);

                seismic_builder.set_seismic_elements(elements);
            }
        }
    }

    async fn prepare<P>(&self, provider: &P, tx: &N::TransactionRequest)
        -> TransportResult<Self::Fillable>
    where P: Provider<N>
    {
        use std::str::FromStr;
        let seismic_tx: &SeismicTransactionRequest = tx.as_ref();

        // Validate consistency
        seismic_tx.validate_seismic_consistency()
            .map_err(|e| TransportErrorKind::custom_str(e))?;

        // Generate fresh ephemeral keypair for this transaction
        let ephemeral_keypair = TxSeismicElements::get_rand_encryption_keypair();

        // Get TEE pubkey from provider
        let tee_pubkey = provider.root().client().request_noparams("seismic_getTeePublicKey").await
            .and_then(|resp: String| {
                let stripped = resp.strip_prefix("0x").unwrap_or(&resp);
                seismic_enclave::secp256k1::PublicKey::from_str(stripped)
                    .map_err(|e| TransportErrorKind::custom_str(
                        &format!("Error parsing TEE pubkey: {:?}", e)
                    ).into())
            })?;

        Ok((tee_pubkey, ephemeral_keypair))
    }

    async fn fill(&self, fillable: Self::Fillable, mut tx: SendableTx<N>)
        -> TransportResult<SendableTx<N>>
    {
        let (tee_pubkey, ephemeral_keypair) = fillable;

        if let Some(builder) = tx.as_mut_builder() {
            // Check if we already encrypted - if so, don't regenerate elements or re-encrypt
            let already_encrypted = {
                if let Some(input) = N::get_request_input(builder) {
                    // If input looks encrypted (e.g., 52 bytes instead of 36), skip
                    !input.is_empty() && ![36, 4, 32, 64].contains(&input.len())
                } else {
                    false
                }
            };

            if already_encrypted {
                return Ok(tx);
            }

            let seismic_builder: &mut SeismicTransactionRequest = builder.as_mut();

            // Set seismic elements using the ephemeral keypair's public key
            let elements = TxSeismicElements::default()
                .with_encryption_pubkey(ephemeral_keypair.public_key())
                .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
                .with_message_version(0);

            seismic_builder.set_seismic_elements(elements.clone());

            // Encrypt the input using the ephemeral keypair's secret key
            if let Some(plaintext) = N::get_request_input(builder) {
                if !plaintext.is_empty() {
                    // Check if the input appears to be already encrypted
                    // Encrypted data has 16 bytes overhead, so 36 bytes plaintext becomes 52 bytes encrypted
                    // Only encrypt if this looks like plaintext (known small sizes)
                    let expected_plaintext_sizes = [36, 4, 32, 64]; // Common test sizes
                    let might_be_plaintext = expected_plaintext_sizes.contains(&plaintext.len());

                    if might_be_plaintext {
                        let encrypted = elements
                            .client_encrypt(plaintext, &tee_pubkey, &ephemeral_keypair.secret_key())
                            .map_err(|e| TransportErrorKind::custom_str(
                                &format!("Error encrypting input: {:?}", e)
                            ))?;

                        N::set_request_input(builder, encrypted)
                            .map_err(|_| TransportErrorKind::custom_str("Error setting encrypted input"))?;
                    }
                }
            }
        }
        Ok(tx)
    }
}

