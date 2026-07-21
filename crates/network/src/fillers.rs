//! Custom fillers for the Seismic provider
//!
//! Normally fillers go in alloy-provider, but we need to put them here because
//! we need it to impl RecommendedFillers for the [`Seismic`] network.

use crate::{seismic_network::SeismicNetwork, wallet::SeismicWallet};
use alloy_consensus::BlockHeader;
use alloy_network::{eip2718::Encodable2718, BlockResponse, Network, TransactionBuilder};
use alloy_network_primitives::HeaderResponse;
use alloy_primitives::{Bytes, U256};
use alloy_provider::{
    fillers::{FillerControlFlow, GasFillable, TxFiller},
    Provider, SendableTx,
};
use alloy_rpc_client::RpcClient;
use alloy_rpc_types_eth::BlockNumberOrTag;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::{InputDecryptionElements, TxSeismicElements};
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use seismic_crypto::secp256k1::{PublicKey, Secp256k1};
use std::str::FromStr;

pub use alloy_provider::fillers::GasFiller;

/// Helper function to fetch TEE public key from a provider
/// This is used when the filler doesn't have a cached TEE pubkey
pub async fn fetch_tee_pubkey<N, P>(provider: &P) -> TransportResult<PublicKey>
where
    N: Network,
    P: Provider<N> + ?Sized,
{
    let resp: String = provider.root().client().request_noparams("seismic_getTeePublicKey").await?;
    let stripped = resp.strip_prefix("0x").unwrap_or(&resp);
    PublicKey::from_str(stripped)
        .map_err(|e| TransportErrorKind::custom_str(&format!("Error parsing TEE pubkey: {:?}", e)))
}

/// Gas filler for the Seismic provider. The node sanitizes `from` on all
/// unsigned `eth_estimateGas` requests to prevent caller-spoofing attacks
/// against msg.sender-gated private state. When a wallet is available, the
/// filler signs the tx before sending to `eth_estimateGas` so the node can
/// authenticate the sender. Without a wallet (RecommendedFillers default),
/// falls back to the standard unsigned GasFiller.
#[derive(Clone, Debug)]
pub struct SeismicGasFiller<N: SeismicNetwork>
where
    N::UnsignedTx: Send + Sync,
{
    inner: GasFiller,
    rpc_url: Option<reqwest::Url>,
    wallet: Option<SeismicWallet<N>>,
}

impl<N: SeismicNetwork> Default for SeismicGasFiller<N>
where
    N::UnsignedTx: Send + Sync,
{
    fn default() -> Self {
        Self { inner: GasFiller::default(), rpc_url: None, wallet: None }
    }
}

impl<N: SeismicNetwork> SeismicGasFiller<N>
where
    N::UnsignedTx: Send + Sync,
{
    /// Create a new SeismicGasFiller with wallet for signed gas estimation.
    pub fn new(rpc_url: reqwest::Url, wallet: SeismicWallet<N>) -> Self {
        Self { inner: GasFiller::default(), rpc_url: Some(rpc_url), wallet: Some(wallet) }
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicGasFiller<N>
where
    <N as Network>::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + InputDecryptionElements,
    <N as Network>::UnsignedTx: Send + Sync,
{
    // (Option<GasFillable>, Option<(u128, u64, RpcClient)>)
    // First: gas values already determined (no estimation needed)
    // Second: (gas_price, block_gas_limit, rpc_client) for signed estimation in fill()
    type Fillable = (Option<GasFillable>, Option<(u128, u64, RpcClient)>);

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        if self.wallet.is_some() {
            // With wallet: treat all txs as legacy for status purposes.
            // Gas estimation is deferred to fill() where we can sign.
            if tx.gas_price().is_some() && tx.gas_limit().is_some() {
                return FillerControlFlow::Finished;
            }
            if tx.max_fee_per_gas().is_some() &&
                tx.max_priority_fee_per_gas().is_some() &&
                tx.gas_limit().is_some()
            {
                return FillerControlFlow::Finished;
            }
            FillerControlFlow::Ready
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
        if self.wallet.is_some() {
            // Seismic transactions cannot be CREATE transactions
            let seismic_tx: &SeismicTransactionRequest = tx.as_ref();
            if seismic_tx.is_seismic() && tx.to().is_none() {
                return Err(TransportErrorKind::custom_str(
                    "Seismic transactions cannot be CREATE transactions (no `to` address). Deploy contracts with regular transactions."
                ).into());
            }

            // Fetch gas_price if not set
            let gas_price = match tx.gas_price() {
                Some(price) => price,
                None => provider.get_gas_price().await?,
            };

            if let Some(limit) = tx.gas_limit() {
                Ok((Some(GasFillable::Legacy { gas_limit: limit, gas_price }), None))
            } else {
                // Defer estimation to fill() where we sign the tx before sending.
                let rpc_url = self.rpc_url.as_ref().ok_or_else(|| {
                    TransportErrorKind::custom_str("RPC URL required for seismic gas estimation")
                })?;
                let client = RpcClient::new_http(rpc_url.clone());
                let latest_block =
                    provider.get_block_by_number(BlockNumberOrTag::Latest).await?.ok_or_else(
                        || TransportErrorKind::custom_str("Failed to fetch latest block"),
                    )?;
                let block_gas_limit = latest_block.header().gas_limit();
                Ok((None, Some((gas_price, block_gas_limit, client))))
            }
        } else {
            // No wallet — fall back to standard unsigned GasFiller
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
            GasFiller::fill(&self.inner, gas_fillable, tx).await
        } else if let Some((gas_price, block_gas_limit, client)) = deferred_estimate {
            let wallet = self.wallet.as_ref().ok_or_else(|| {
                TransportErrorKind::custom_str("Wallet required for seismic gas estimation")
            })?;

            let mut tx_for_estimate = match &tx {
                SendableTx::Builder(builder) => builder.clone(),
                SendableTx::Envelope(_) => {
                    return Err(TransportErrorKind::custom_str(
                        "Cannot estimate gas on already-signed transaction",
                    )
                    .into());
                }
            };

            // Set temporary gas fields so the tx is complete enough to sign
            tx_for_estimate.set_gas_limit(block_gas_limit);
            tx_for_estimate.set_gas_price(gas_price);

            // Sign and send as bytes so the node can authenticate the sender
            let envelope = tx_for_estimate
                .build(wallet)
                .await
                .map_err(|e| TransportErrorKind::custom_str(&format!("{e:?}")))?;
            let encoded_tx = Bytes::from(envelope.encoded_2718());
            let gas: U256 = client.request("eth_estimateGas", (encoded_tx,)).await?;
            let gas_limit: u64 = gas
                .try_into()
                .map_err(|_| TransportErrorKind::custom_str("Gas estimate exceeds u64::MAX"))?;

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
/// Generates one provider keypair for the client that is reused for all transactions.
/// This combines element generation and encryption into a single filler
/// to avoid the complexity of sharing provider state between separate fillers.
#[derive(Clone, Debug)]
pub struct SeismicElementsFiller {
    /// Cached TEE public key (fetched once at provider creation, or provided directly)
    tee_pubkey: Option<PublicKey>,
    /// Custom blocks window for transaction expiration (overrides BLOCKS_WINDOW)
    blocks_window: Option<u64>,
    /// Client's provider secret key for encryption/decryption (generated once at client creation)
    provider_secret_key: seismic_crypto::secp256k1::SecretKey,
    /// Whether seismic calls should be marked as signed_read (true for signed providers)
    signed_read: bool,
}

impl SeismicElementsFiller {
    /// Create a new SeismicElementsFiller with RPC URL (signed_read defaults to false)
    pub fn new() -> Self {
        let provider_keypair = TxSeismicElements::get_rand_encryption_keypair();
        Self {
            tee_pubkey: None,
            blocks_window: None,
            provider_secret_key: provider_keypair.secret_key().clone(),
            signed_read: false,
        }
    }

    /// Create with a cached TEE pubkey (avoids fetching per-transaction, signed_read
    /// defaults to false)
    pub fn with_tee_pubkey(tee_pubkey: PublicKey) -> Self {
        let provider_keypair = TxSeismicElements::get_rand_encryption_keypair();
        Self {
            tee_pubkey: Some(tee_pubkey),
            blocks_window: None,
            provider_secret_key: provider_keypair.secret_key().clone(),
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

    /// Get the provider secret key for response decryption
    pub fn provider_secret_key(&self) -> &seismic_crypto::secp256k1::SecretKey {
        &self.provider_secret_key
    }

    /// Check whether a transaction has already been encrypted by this filler.
    ///
    /// We detect this by comparing the `encryption_pubkey` in the elements to our
    /// ephemeral public key. Users never set the pubkey directly (it's derived from
    /// the filler's provider secret key), so a match means we already ran.
    fn is_encrypted(&self, tx: &SeismicTransactionRequest) -> bool {
        let our_pubkey = self.provider_secret_key.public_key(&Secp256k1::new());
        tx.seismic_elements.as_ref().map_or(false, |e| e.encryption_pubkey == our_pubkey)
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicElementsFiller
where
    N::TransactionRequest: AsRef<SeismicTransactionRequest>
        + AsMut<SeismicTransactionRequest>
        + InputDecryptionElements,
    N::UnsignedTx: Send + Sync,
{
    // Fillable contains: Some((tee_pubkey, provider_secret_key, elements, plaintext)) for fill()
    // to encrypt
    type Fillable = Option<(
        PublicKey,
        seismic_crypto::secp256k1::SecretKey,
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
                // Encryption is complete when the encryption_pubkey matches our ephemeral key.
                // We can't use encryption_nonce != 0 as a sentinel because users can now
                // set custom nonces via SecurityParams.
                if self.is_encrypted(seismic_tx) {
                    FillerControlFlow::Finished
                } else {
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

        // If encryption already happened (our pubkey is set), we're done
        if seismic_tx.is_seismic() && self.is_encrypted(seismic_tx) {
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

        // Use the client's provider secret key (generated once at client creation)
        let provider_secret_key = self.provider_secret_key.clone();

        // Derive public key from secret key
        let provider_pubkey = provider_secret_key.public_key(&Secp256k1::new());

        // Get TEE public key (use cached if available, otherwise fetch via RPC)
        let tee_pubkey = if let Some(cached) = &self.tee_pubkey {
            *cached
        } else {
            fetch_tee_pubkey(provider).await?
        };

        // Extract user-provided overrides from partial elements (if any)
        let partial = seismic_tx.seismic_elements.as_ref();
        let user_recent_block_hash = partial.map(|e| e.recent_block_hash).filter(|h| !h.is_zero());
        let user_encryption_nonce =
            partial.map(|e| e.encryption_nonce).filter(|n| *n != alloy_primitives::Uint::ZERO);
        let user_expires_at = partial.map(|e| e.expires_at_block).filter(|&b| b > 0);
        let signed_read = partial.map_or(self.signed_read, |e| e.signed_read);
        let message_version = partial.map(|e| e.message_version).filter(|&v| v > 0).unwrap_or(0);

        // Fetch block info only if we need recent_block_hash or expires_at_block
        let (recent_block_hash, latest_block) =
            if user_recent_block_hash.is_some() && user_expires_at.is_some() {
                // User provided both — skip the RPC call entirely
                (user_recent_block_hash.unwrap(), 0)
            } else {
                let block = provider
                    .get_block_by_number(BlockNumberOrTag::Latest)
                    .await
                    .map_err(|_| TransportErrorKind::custom_str("Failed to fetch recent block"))?
                    .ok_or_else(|| TransportErrorKind::custom_str("Block not found"))?;
                let header = block.header();
                (user_recent_block_hash.unwrap_or_else(|| header.hash()), header.number())
            };

        let expires_at_block = user_expires_at.unwrap_or_else(|| {
            let window = self.blocks_window.unwrap_or(BLOCKS_WINDOW);
            latest_block + window
        });

        let encryption_nonce =
            user_encryption_nonce.unwrap_or_else(TxSeismicElements::get_rand_encryption_nonce);

        // Create seismic elements (without full metadata yet, will encrypt in fill())
        let elements = TxSeismicElements::default()
            .with_encryption_pubkey(provider_pubkey)
            .with_encryption_nonce(encryption_nonce)
            .with_message_version(message_version)
            .with_recent_block_hash(recent_block_hash)
            .with_expires_at_block(expires_at_block)
            .with_signed_read(signed_read);

        // Return data needed for encryption in fill() (when nonce will be available)
        Ok(Some((tee_pubkey, provider_secret_key, elements, plaintext)))
    }

    async fn fill(
        &self,
        fillable: Self::Fillable,
        mut tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        // If None, no encryption needed
        let Some((tee_pubkey, provider_secret_key, elements, plaintext)) = fillable else {
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
                metadata.client_encrypt(&plaintext, &tee_pubkey, &provider_secret_key).map_err(
                    |e| TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e)),
                )?;

            // Set encrypted input
            N::set_request_input(builder, encrypted)
                .map_err(|_| TransportErrorKind::custom_str("Error setting encrypted input"))?;
        }
        Ok(tx)
    }
}
