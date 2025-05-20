use alloy_rpc_types_eth::{state::StateOverride, Block, BlockOverrides, Log, TransactionRequest};
use alloc::{string::String, vec::Vec};
use alloy_primitives::Bytes;

/// Represents a batch of calls to be simulated sequentially within a block.
/// This struct includes block and state overrides as well as the transaction requests to be
/// executed.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct SimBlock<T> {
    /// Modifications to the default block characteristics.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub block_overrides: Option<BlockOverrides>,
    /// State modifications to apply before executing the transactions.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub state_overrides: Option<StateOverride>,
    /// A vector of transactions to be simulated.
    #[cfg_attr(feature = "serde", serde(default))]
    pub calls: Vec<T>,
}

impl<T> SimBlock<T> {
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
    pub fn call(mut self, call: T) -> Self {
        self.calls.push(call);
        self
    }

    /// Adds multiple calls to the block.
    pub fn extend_calls(mut self, calls: impl IntoIterator<Item = T>) -> Self {
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
pub struct SimulatePayload<T> {
    /// Array of block state calls to be executed at specific, optional block/state.
    #[cfg_attr(feature = "serde", serde(default))]
    pub block_state_calls: Vec<SimBlock<T>>,
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

impl<T> SimulatePayload<T> {
    /// Adds a block to the simulation payload.
    pub fn extend(mut self, block: SimBlock<T>) -> Self {
        self.block_state_calls.push(block);
        self
    }

    /// Adds multiple blocks to the simulation payload.
    pub fn extend_blocks(mut self, blocks: impl IntoIterator<Item = SimBlock<T>>) -> Self {
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