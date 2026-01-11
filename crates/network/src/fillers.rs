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
use seismic_alloy_consensus::{InputDecryptionElements, TxSeismicElements};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use std::str::FromStr;

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
            // For seismic transactions, treat them like legacy transactions
            // Encryption should have already happened in SeismicElementsFiller.fill_sync()
            // so we can safely call estimate_gas
            let gas_price = match tx.gas_price() {
                Some(price) => price,
                None => provider.get_gas_price().await?,
            };

            let gas_limit = match tx.gas_limit() {
                Some(limit) => limit,
                None => provider.estimate_gas(tx.clone()).await?,
            };

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
#[derive(Clone, Debug)]
pub struct SeismicElementsFiller {
    /// Cached TEE public key (fetched once at provider creation)
    tee_pubkey: Option<seismic_enclave::secp256k1::PublicKey>,
}

impl Default for SeismicElementsFiller {
    fn default() -> Self {
        Self { tee_pubkey: None }
    }
}

impl SeismicElementsFiller {
    /// Create a new SeismicElementsFiller without TEE pubkey (will fetch lazily)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new SeismicElementsFiller with a cached TEE pubkey
    pub fn with_tee_pubkey(tee_pubkey: seismic_enclave::secp256k1::PublicKey) -> Self {
        Self { tee_pubkey: Some(tee_pubkey) }
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
            let has_input = N::get_request_input(tx).map_or(false, |i| !i.is_empty());

            if has_input {
                // If elements are set, encryption is complete
                if seismic_tx.seismic_elements.is_some() {
                    FillerControlFlow::Finished
                } else {
                    // No elements yet - needs encryption
                    FillerControlFlow::Ready
                }
            } else {
                // Empty input, nothing to encrypt
                FillerControlFlow::Finished
            }
        } else {
            FillerControlFlow::Finished
        }
    }

    fn fill_sync(&self, tx: &mut SendableTx<N>) {
        // If we have a cached TEE pubkey, encrypt synchronously here
        // This ensures encryption happens before GasFiller.prepare() runs
        if let Some(tee_pubkey) = &self.tee_pubkey {
            if let Some(builder) = tx.as_mut_builder() {
                let seismic_builder: &mut SeismicTransactionRequest = builder.as_mut();

                // Only process if this is a seismic transaction without elements yet
                if seismic_builder.is_seismic() && seismic_builder.seismic_elements.is_none() {
                    // Generate fresh ephemeral keypair for this transaction
                    let ephemeral_keypair = TxSeismicElements::get_rand_encryption_keypair();

                    // Set seismic elements
                    let elements = TxSeismicElements::default()
                        .with_encryption_pubkey(ephemeral_keypair.public_key())
                        .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
                        .with_message_version(0);

                    seismic_builder.set_seismic_elements(elements.clone());

                    // Encrypt the input (always assume plaintext)
                    if let Some(plaintext) = N::get_request_input(builder) {
                        if !plaintext.is_empty() {
                            if let Ok(encrypted) = elements.client_encrypt(
                                plaintext,
                                tee_pubkey,
                                &ephemeral_keypair.secret_key(),
                            ) {
                                let _ = N::set_request_input(builder, encrypted);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn prepare<P>(&self, provider: &P, tx: &N::TransactionRequest)
        -> TransportResult<Self::Fillable>
    where
        P: Provider<N>,
    {
        let seismic_tx: &SeismicTransactionRequest = tx.as_ref();

        // Validate consistency
        seismic_tx.validate_seismic_consistency()
            .map_err(|e| TransportErrorKind::custom_str(e))?;

        // If encryption already happened in fill_sync(), we're done
        if seismic_tx.is_seismic() && seismic_tx.seismic_elements.is_some() {
            // Return dummy values since encryption is already complete
            let dummy_keypair = TxSeismicElements::get_rand_encryption_keypair();
            let dummy_pubkey = seismic_enclave::get_unsecure_sample_secp256k1_pk();
            return Ok((dummy_pubkey, dummy_keypair));
        }

        // Generate fresh ephemeral keypair for this transaction
        let ephemeral_keypair = TxSeismicElements::get_rand_encryption_keypair();

        // Get TEE public key from RPC (only if we didn't cache it)
        let tee_pubkey = provider.root().client()
            .request_noparams("seismic_getTeePublicKey")
            .await
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
            let seismic_builder: &mut SeismicTransactionRequest = builder.as_mut();

            // If elements are already set (from fill_sync), skip encryption
            if seismic_builder.seismic_elements.is_some() {
                return Ok(tx);
            }

            // Set seismic elements using the real ephemeral keypair's public key
            let elements = TxSeismicElements::default()
                .with_encryption_pubkey(ephemeral_keypair.public_key())
                .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
                .with_message_version(0);

            seismic_builder.set_seismic_elements(elements.clone());

            // Encrypt the input using the ephemeral keypair's secret key
            // We always assume the input is plaintext (no heuristics)
            if let Some(plaintext) = N::get_request_input(builder) {
                if !plaintext.is_empty() {
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
        Ok(tx)
    }
}

