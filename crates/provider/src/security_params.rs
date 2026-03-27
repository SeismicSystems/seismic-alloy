//! Per-call overrides for seismic encryption parameters.
use alloy_primitives::{aliases::U96, B256};
use seismic_alloy_consensus::TxSeismicElements;

/// Per-call overrides for seismic encryption parameters.
///
/// All fields are optional — unset fields use the filler's defaults
/// (random nonce, latest block hash, current block + 100 for expiration).
///
/// # Example
///
/// ```rust,ignore
/// let result = provider
///     .seismic_call_with(addr, MyContract::isOddCall {},
///         SecurityParams::default().expires_at(current_block + 10))
///     .await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct SecurityParams {
    /// Block number after which this transaction expires.
    pub expires_at_block: Option<u64>,
    /// Recent block hash for chain-state pinning.
    pub recent_block_hash: Option<B256>,
    /// Custom AEAD nonce (testing only — reusing a (key, nonce) pair breaks encryption).
    pub encryption_nonce: Option<U96>,
}

impl SecurityParams {
    /// Set the block number after which this transaction expires.
    pub fn expires_at(mut self, block: u64) -> Self {
        self.expires_at_block = Some(block);
        self
    }

    /// Set the recent block hash for chain-state pinning.
    pub fn recent_block_hash(mut self, hash: B256) -> Self {
        self.recent_block_hash = Some(hash);
        self
    }

    /// Set a custom encryption nonce (testing only).
    pub fn encryption_nonce(mut self, nonce: U96) -> Self {
        self.encryption_nonce = Some(nonce);
        self
    }

    /// Apply these params to a `TxSeismicElements`.
    pub(crate) fn apply_to(&self, elements: &mut TxSeismicElements) {
        if let Some(block) = self.expires_at_block {
            elements.expires_at_block = block;
        }
        if let Some(hash) = self.recent_block_hash {
            elements.recent_block_hash = hash;
        }
        if let Some(nonce) = self.encryption_nonce {
            elements.encryption_nonce = nonce;
        }
    }
}
