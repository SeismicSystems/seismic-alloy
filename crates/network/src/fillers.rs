//! Custom fillers for the Seismic provider
//!
//! Normally fillers go in alloy-provider, but we need to put them here because
//! we need it to impl RecommendedFillers for the [`Seismic`] network.

use crate::seismic_network::SeismicNetwork;
use alloy_consensus::BlockHeader;
use alloy_network::{BlockResponse, Network, TransactionBuilder};
use alloy_network_primitives::HeaderResponse;
use alloy_provider::{
    fillers::{FillerControlFlow, GasFillable, TxFiller},
    Provider, ProviderBuilder, SendableTx,
};
use alloy_rpc_client::RpcClient;
use alloy_rpc_types_eth::BlockNumberOrTag;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::{InputDecryptionElements, TxSeismicElements};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use seismic_enclave::secp256k1::PublicKey;
use std::str::FromStr;

pub use alloy_provider::fillers::GasFiller;

/// Helper function to fetch TEE public key from a provider
/// This is used when the filler doesn't have a cached TEE pubkey
pub async fn fetch_tee_pubkey<N, P>(provider: &P) -> TransportResult<PublicKey>
where
    N: Network,
    P: Provider<N> + ?Sized,
{
    let resp: String = provider
        .root()
        .client()
        .request_noparams("seismic_getTeePublicKey")
        .await?;
    let stripped = resp.strip_prefix("0x").unwrap_or(&resp);
    PublicKey::from_str(stripped).map_err(|e| {
        TransportErrorKind::custom_str(&format!("Error parsing TEE pubkey: {:?}", e))
    })
}

/// A wrapper for alloy_provider::fillers::GasFiller that handles gas for seismic transactions
/// Seismic tx are treated like legacy transactions
#[derive(Clone, Debug)]
pub struct SeismicGasFiller {
    inner: GasFiller,
    rpc_url: Option<reqwest::Url>,
}

impl Default for SeismicGasFiller {
    fn default() -> Self {
        Self { inner: GasFiller::default(), rpc_url: None }
    }
}

impl SeismicGasFiller {
    /// Create a new SeismicGasFiller with RPC URL for gas estimation in fill() phase
    pub fn with_url(rpc_url: reqwest::Url) -> Self {
        Self { inner: GasFiller::default(), rpc_url: Some(rpc_url) }
    }
}

impl SeismicGasFiller {
    fn is_seismic_tx<N>(&self, tx: &N::TransactionRequest) -> bool
    where
        N: SeismicNetwork,
        N::TransactionRequest: InputDecryptionElements,
        <N as Network>::UnsignedTx: Send + Sync,
    {
        // Only treat as seismic if elements are actually set
        // This ensures estimate_gas is called on regular tx during prepare() phase
        tx.get_decryption_elements().is_ok()
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicGasFiller
where
    <N as Network>::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + InputDecryptionElements,
    <N as Network>::UnsignedTx: Send + Sync,
{
    // (Option<GasFillable>, Option<(u128, reqwest::Url)>)
    // First: Gas values if already set
    // Second: (gas_price, rpc_url) for deferred estimation in fill() for seismic tx
    type Fillable = (Option<GasFillable>, Option<(u128, reqwest::Url)>);

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        if self.is_seismic_tx::<N>(tx) {
            // Seismic transaction - treat like legacy
            if tx.gas_price().is_some() && tx.gas_limit().is_some() {
                return FillerControlFlow::Finished;
            } else {
                return FillerControlFlow::Ready;
            }
        } else {
            <GasFiller as TxFiller<N>>::status(&self.inner, tx)
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
        // Check if transaction is marked as seismic (by tx type or has elements)
        let seismic_tx: &SeismicTransactionRequest = tx.as_ref();
        let is_marked_seismic = seismic_tx.is_seismic();

        if is_marked_seismic {
            // Seismic transactions cannot be CREATE transactions
            if tx.to().is_none() {
                return Err(TransportErrorKind::custom_str(
                    "Seismic transactions cannot be CREATE transactions (no `to` address). Deploy contracts with regular transactions."
                ).into());
            }

            // For seismic transactions, always defer gas estimation to fill() phase
            // (after encryption is done by SeismicElementsFiller)

            // Fetch gas_price if not set
            let gas_price = match tx.gas_price() {
                Some(price) => price,
                None => provider.get_gas_price().await?,
            };

            // Check if gas_limit is already set
            if let Some(limit) = tx.gas_limit() {
                // Gas limit already set, no need to estimate
                Ok((Some(GasFillable::Legacy { gas_limit: limit, gas_price }), None))
            } else {
                // Defer estimation to fill() - need RPC URL
                let rpc_url = self.rpc_url.as_ref().ok_or_else(|| {
                    TransportErrorKind::custom_str("RPC URL required for seismic gas estimation")
                })?;
                Ok((None, Some((gas_price, rpc_url.clone()))))
            }
        } else {
            // For non-seismic transactions, use regular GasFiller logic
            Ok((Some(GasFiller::prepare(&self.inner, provider, tx).await?), None))
        }
    }

    async fn fill(
        &self,
        fillable: Self::Fillable,
        tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        let (immediate_fill, deferred_estimate) = fillable;

        if let Some(gas_fillable) = immediate_fill {
            // Gas values already determined in prepare()
            GasFiller::fill(&self.inner, gas_fillable, tx).await
        } else if let Some((gas_price, rpc_url)) = deferred_estimate {
            // Need to estimate gas now (after encryption)
            let tx_for_estimate = match &tx {
                SendableTx::Builder(builder) => {
                    // Clone the builder's transaction for estimation
                    builder.clone()
                }
                SendableTx::Envelope(_) => {
                    // Already signed, can't estimate
                    return Err(TransportErrorKind::custom_str(
                        "Cannot estimate gas on already-signed transaction",
                    )
                    .into());
                }
            };

            // Create temporary provider for estimate_gas call
            let client = RpcClient::new_http(rpc_url);
            let temp_provider =
                ProviderBuilder::<_, _, N>::default().network::<N>().connect_client(client);

            let gas_limit = temp_provider.estimate_gas(tx_for_estimate).await?;
            let gas_fillable = GasFillable::Legacy { gas_limit, gas_price };

            GasFiller::fill(&self.inner, gas_fillable, tx).await
        } else {
            Err(TransportErrorKind::custom_str(
                "Invalid fillable state: neither immediate nor deferred gas estimation available",
            )
            .into())
        }
    }
}

/// Default number of blocks to add to current block for transaction expiration
pub const BLOCKS_WINDOW: u64 = 100;

/// Generates seismic elements and encrypts transaction input.
/// Generates one ephemeral keypair for the client that is reused for all transactions.
/// This combines element generation and encryption into a single filler
/// to avoid the complexity of sharing ephemeral state between separate fillers.
#[derive(Clone, Debug)]
pub struct SeismicElementsFiller {
    /// Cached TEE public key (fetched once at provider creation, or provided directly)
    tee_pubkey: Option<PublicKey>,
    /// Custom blocks window for transaction expiration (overrides BLOCKS_WINDOW)
    blocks_window: Option<u64>,
    /// Client's ephemeral secret key for encryption/decryption (generated once at client creation)
    ephemeral_secret_key: seismic_enclave::secp256k1::SecretKey,
    /// Whether seismic calls should be marked as signed_read (true for signed providers)
    signed_read: bool,
}

impl SeismicElementsFiller {
    /// Create a new SeismicElementsFiller with RPC URL (signed_read defaults to false)
    pub fn new() -> Self {
        let ephemeral_keypair = TxSeismicElements::get_rand_encryption_keypair();
        Self {
            tee_pubkey: None,
            blocks_window: None,
            ephemeral_secret_key: ephemeral_keypair.secret_key().clone(),
            signed_read: false,
        }
    }

    /// Create with a cached TEE pubkey and RPC URL (avoids fetching per-transaction, signed_read
    /// defaults to false)
    pub fn with_tee_pubkey_and_url(tee_pubkey: PublicKey) -> Self {
        let ephemeral_keypair = TxSeismicElements::get_rand_encryption_keypair();
        Self {
            tee_pubkey: Some(tee_pubkey),
            blocks_window: None,
            ephemeral_secret_key: ephemeral_keypair.secret_key().clone(),
            signed_read: false,
        }
    }

    /// Set whether seismic calls should be marked as signed_read
    pub fn with_signed_read(mut self, signed_read: bool) -> Self {
        self.signed_read = signed_read;
        self
    }

    /// Set a custom blocks window for transaction expiration
    pub fn with_blocks_window(mut self, blocks_window: u64) -> Self {
        self.blocks_window = Some(blocks_window);
        self
    }

    /// Get the ephemeral secret key for response decryption
    pub fn ephemeral_secret_key(&self) -> &seismic_enclave::secp256k1::SecretKey {
        &self.ephemeral_secret_key
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicElementsFiller
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
{
    // Fillable contains: Some((tee_pubkey, ephemeral_secret_key, elements, plaintext)) for fill()
    // to encrypt
    type Fillable = Option<(
        PublicKey,
        seismic_enclave::secp256k1::SecretKey,
        TxSeismicElements,
        alloy_primitives::Bytes,
    )>;

    fn status(&self, tx: &N::TransactionRequest) -> FillerControlFlow {
        let seismic_tx: &SeismicTransactionRequest = tx.as_ref();

        // Validate consistency first
        if let Err(_e) = seismic_tx.validate_seismic_consistency() {
            // Return ready so we can error in prepare()
            return FillerControlFlow::Ready;
        }

        // Check if this is a seismic transaction that needs encryption
        if seismic_tx.is_seismic() {
            // Seismic transactions cannot be CREATE transactions
            if tx.to().is_none() {
                // Return ready so we can error in prepare()
                return FillerControlFlow::Ready;
            }

            let has_input = N::get_request_input(tx).map_or(false, |i| !i.is_empty());

            if has_input {
                // If elements are set AND encryption_nonce is non-zero, encryption is complete
                if seismic_tx.seismic_elements.as_ref().map_or(false, |e| e.encryption_nonce != 0) {
                    FillerControlFlow::Finished
                } else {
                    // No elements yet or incomplete elements - needs encryption
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

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {
        // All seismic element generation and encryption happens in prepare()
    }

    async fn prepare<P>(
        &self,
        provider: &P,
        tx: &N::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<N>,
    {
        let seismic_tx: &SeismicTransactionRequest = tx.as_ref();

        // Validate consistency
        seismic_tx.validate_seismic_consistency().map_err(|e| TransportErrorKind::custom_str(e))?;

        // If encryption already happened (encryption_nonce is non-zero), we're done
        if seismic_tx.is_seismic() &&
            seismic_tx.seismic_elements.as_ref().map_or(false, |e| e.encryption_nonce != 0)
        {
            return Ok(None);
        }

        // If this is not a seismic tx or has no input, skip
        if !seismic_tx.is_seismic() {
            return Ok(None);
        }

        // Seismic transactions cannot be CREATE transactions
        if tx.to().is_none() {
            return Err(TransportErrorKind::custom_str(
                "Cannot encrypt calldata for CREATE transactions. Seismic transactions must have a `to` address."
            ).into());
        }

        // Get plaintext input - if empty, no encryption needed
        let Some(plaintext) = N::get_request_input(tx) else {
            return Ok(None);
        };
        if plaintext.is_empty() {
            return Ok(None);
        }
        let plaintext = plaintext.clone();

        // Use the client's ephemeral secret key (generated once at client creation)
        let ephemeral_secret_key = self.ephemeral_secret_key.clone();

        // Derive public key from secret key
        let secp = seismic_enclave::secp256k1::Secp256k1::new();
        let ephemeral_pubkey = ephemeral_secret_key.public_key(&secp);

        // Get TEE public key (use cached if available, otherwise fetch via RPC)
        let tee_pubkey = if let Some(cached) = &self.tee_pubkey {
            *cached
        } else {
            fetch_tee_pubkey(provider).await?
        };

        // Get latest block number for expiration calculation

        // Get recent block hash (one block behind for finalization)
        let block = provider
            .get_block_by_number(BlockNumberOrTag::Latest)
            .await
            .map_err(|_| TransportErrorKind::custom_str("Failed to fetch recent block"))?
            .ok_or_else(|| TransportErrorKind::custom_str("Block not found"))?;
        let block_header = block.header();
        let recent_block_hash = block_header.hash();
        let latest_block = block_header.number();

        // Calculate expires_at_block and get signed_read from existing partial elements if present
        let (expires_at_block, signed_read) = if let Some(elements) = &seismic_tx.seismic_elements {
            let expires = if elements.expires_at_block > 0 {
                elements.expires_at_block // User manually set it
            } else {
                let window = self.blocks_window.unwrap_or(BLOCKS_WINDOW);
                latest_block + window
            };
            (expires, elements.signed_read) // Preserve signed_read from partial elements
        } else {
            let window = self.blocks_window.unwrap_or(BLOCKS_WINDOW);
            (latest_block + window, self.signed_read) // Use filler default
        };

        // Create seismic elements (without full metadata yet, will encrypt in fill())
        let elements = TxSeismicElements::default()
            .with_encryption_pubkey(ephemeral_pubkey)
            .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
            .with_message_version(0)
            .with_recent_block_hash(recent_block_hash)
            .with_expires_at_block(expires_at_block)
            .with_signed_read(signed_read);

        // Return data needed for encryption in fill() (when nonce will be available)
        Ok(Some((tee_pubkey, ephemeral_secret_key, elements, plaintext)))
    }

    async fn fill(
        &self,
        fillable: Self::Fillable,
        mut tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        // If None, no encryption needed
        let Some((tee_pubkey, ephemeral_secret_key, elements, plaintext)) = fillable else {
            return Ok(tx);
        };

        // Apply the elements and encrypt with metadata (nonce is now available)
        if let Some(builder) = tx.as_mut_builder() {
            let seismic_builder: &mut SeismicTransactionRequest = builder.as_mut();

            // Set seismic elements first
            seismic_builder.set_seismic_elements(elements);

            // Now create metadata with sender (nonce and chain_id are now set by other fillers)
            let sender = builder.from().ok_or_else(|| {
                TransportErrorKind::custom_str(
                    "Sender address required for seismic transaction encryption",
                )
            })?;

            let metadata = builder.metadata(sender).map_err(|e| {
                TransportErrorKind::custom_str(&format!("Error creating metadata: {:?}", e))
            })?;

            // Encrypt using metadata.client_encrypt()
            let encrypted =
                metadata.client_encrypt(&plaintext, &tee_pubkey, &ephemeral_secret_key).map_err(
                    |e| TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e)),
                )?;

            // Set encrypted input
            N::set_request_input(builder, encrypted)
                .map_err(|_| TransportErrorKind::custom_str("Error setting encrypted input"))?;
        }
        Ok(tx)
    }
}
