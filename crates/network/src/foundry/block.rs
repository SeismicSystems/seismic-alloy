//! Seismic overrides to types commonly used in foundry
use std::ops::{Deref, DerefMut};

use alloy_consensus::error::ValueError;
use alloy_network::{AnyHeader, AnyRpcHeader, Network};
use alloy_network_primitives::BlockResponse;
use alloy_rpc_types_eth::{state::StateOverride, Block, BlockOverrides, BlockTransactions};
use alloy_serde::WithOtherFields;
use derive_more::From;
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use serde::{Deserialize, Serialize};

use crate::foundry::{tx_request::SeismicFoundryRpcTransaction, SeismicFoundry};

type SeismicFoundryRpcBlockInner = Block<SeismicFoundryRpcTransaction, AnyRpcHeader>;

/// Seismic RPC block with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeismicFoundryRpcBlock(pub WithOtherFields<SeismicFoundryRpcBlockInner>);

impl SeismicFoundryRpcBlock {
    /// Create a new [`SeismicFoundryRpcBlock`].
    pub const fn new(inner: WithOtherFields<SeismicFoundryRpcBlockInner>) -> Self {
        Self(inner)
    }

    /// Consumes the type and returns the wrapped rpc block.
    pub fn into_inner(self) -> SeismicFoundryRpcBlockInner {
        self.0.into_inner()
    }

    /// Tries to convert inner transactions into a vector of [`SeismicFoundryRpcTransaction`].
    ///
    /// Returns an error if the block contains only transaction hashes or if it is an uncle block.
    pub fn try_into_transactions(
        self,
    ) -> Result<
        Vec<SeismicFoundryRpcTransaction>,
        ValueError<BlockTransactions<SeismicFoundryRpcTransaction>>,
    > {
        self.0.inner.try_into_transactions()
    }

    /// Consumes the type and returns an iterator over the transactions in this block
    pub fn into_transactions_iter(self) -> impl Iterator<Item = SeismicFoundryRpcTransaction> {
        self.into_inner().transactions.into_transactions()
    }
}

impl BlockResponse for SeismicFoundryRpcBlock {
    type Header = <SeismicFoundry as Network>::HeaderResponse;
    type Transaction = <SeismicFoundry as Network>::TransactionResponse;

    fn header(&self) -> &Self::Header {
        &self.0.inner.header
    }

    fn transactions(&self) -> &BlockTransactions<Self::Transaction> {
        &self.0.inner.transactions
    }

    fn transactions_mut(&mut self) -> &mut BlockTransactions<Self::Transaction> {
        &mut self.0.inner.transactions
    }

    fn other_fields(&self) -> Option<&alloy_serde::OtherFields> {
        self.0.other_fields()
    }
}

impl AsRef<WithOtherFields<SeismicFoundryRpcBlockInner>> for SeismicFoundryRpcBlock {
    fn as_ref(&self) -> &WithOtherFields<SeismicFoundryRpcBlockInner> {
        &self.0
    }
}

impl Deref for SeismicFoundryRpcBlock {
    type Target = WithOtherFields<SeismicFoundryRpcBlockInner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SeismicFoundryRpcBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<SeismicFoundryRpcBlockInner> for SeismicFoundryRpcBlock {
    fn from(value: SeismicFoundryRpcBlockInner) -> Self {
        let block = value.map_header(|h| h.map(|h| AnyHeader { ..h.into() }));
        Self(WithOtherFields::new(block))
    }
}

impl From<SeismicFoundryRpcBlock> for SeismicFoundryRpcBlockInner {
    fn from(value: SeismicFoundryRpcBlock) -> Self {
        value.into_inner()
    }
}

impl From<SeismicFoundryRpcBlock> for WithOtherFields<SeismicFoundryRpcBlockInner> {
    fn from(value: SeismicFoundryRpcBlock) -> Self {
        value.0
    }
}

/// Represents a batch of calls to be simulated sequentially within a block.
/// This struct includes block and state overrides as well as the transaction requests to be
/// executed.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct SeismicFoundrySimBlock {
    /// Modifications to the default block characteristics.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub block_overrides: Option<BlockOverrides>,
    /// State modifications to apply before executing the transactions.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub state_overrides: Option<StateOverride>,
    /// A vector of transactions to be simulated.
    #[cfg_attr(feature = "serde", serde(default))]
    pub calls: Vec<SeismicTransactionRequest>,
}

impl SeismicFoundrySimBlock {
    /// Enables state overrides
    pub fn with_state_overrides(mut self, overrides: StateOverride) -> Self {
        self.state_overrides = Some(overrides);
        self
    }

    /// Enables block overrides
    pub fn with_block_overrides(mut self, overrides: BlockOverrides) -> Self {
        self.block_overrides = Some(overrides);
        self
    }

    /// Adds a call to the block.
    pub fn call(mut self, call: SeismicTransactionRequest) -> Self {
        self.calls.push(call);
        self
    }

    /// Adds multiple calls to the block.
    pub fn extend_calls(
        mut self,
        calls: impl IntoIterator<Item = SeismicTransactionRequest>,
    ) -> Self {
        self.calls.extend(calls);
        self
    }
}

/// Simulation options for executing multiple blocks and transactions.
///
/// This struct configures how simulations are executed, including whether to trace token transfers,
/// validate transaction sequences, and whether to return full transaction objects.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct SeismicFoundrySimulatePayload {
    /// Array of block state calls to be executed at specific, optional block/state.
    #[cfg_attr(feature = "serde", serde(default))]
    pub block_state_calls: Vec<SeismicFoundrySimBlock>,
    /// Flag to determine whether to trace ERC20/ERC721 token transfers within transactions.
    #[cfg_attr(feature = "serde", serde(default))]
    pub trace_transfers: bool,
    /// Flag to enable or disable validation of the transaction sequence in the blocks.
    #[cfg_attr(feature = "serde", serde(default))]
    pub validation: bool,
    /// Flag to decide if full transactions should be returned instead of just their hashes.
    #[cfg_attr(feature = "serde", serde(default))]
    pub return_full_transactions: bool,
}

impl SeismicFoundrySimulatePayload {
    /// Adds a block to the simulation payload.
    pub fn extend(mut self, block: SeismicFoundrySimBlock) -> Self {
        self.block_state_calls.push(block);
        self
    }

    /// Adds multiple blocks to the simulation payload.
    pub fn extend_blocks(
        mut self,
        blocks: impl IntoIterator<Item = SeismicFoundrySimBlock>,
    ) -> Self {
        self.block_state_calls.extend(blocks);
        self
    }

    /// Enables tracing of token transfers.
    pub const fn with_trace_transfers(mut self) -> Self {
        self.trace_transfers = true;
        self
    }

    /// Enables validation of the transaction sequence.
    pub const fn with_validation(mut self) -> Self {
        self.validation = true;
        self
    }

    /// Enables returning full transactions.
    pub const fn with_full_transactions(mut self) -> Self {
        self.return_full_transactions = true;
        self
    }
}
