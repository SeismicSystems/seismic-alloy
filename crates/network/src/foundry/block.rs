//! Seismic overrides to types commonly used in foundry
use alloy_consensus::error::ValueError;
use alloy_network::{AnyRpcHeader, Network};
use alloy_network_primitives::BlockResponse;
use alloy_rpc_types_eth::{Block, BlockTransactions};
use alloy_serde::WithOtherFields;
use derive_more::From;
use serde::{Deserialize, Serialize};

use crate::foundry::{tx_request::SeismicFoundryRpcTransaction, SeismicFoundry};

/// Seismic RPC block with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeismicFoundryRpcBlock(
    pub WithOtherFields<Block<SeismicFoundryRpcTransaction, AnyRpcHeader>>,
);

impl SeismicFoundryRpcBlock {
    /// Create a new [`SeismicFoundryRpcBlock`].
    pub const fn new(
        inner: WithOtherFields<Block<SeismicFoundryRpcTransaction, AnyRpcHeader>>,
    ) -> Self {
        Self(inner)
    }

    /// Consumes the type and returns the wrapped rpc block.
    pub fn into_inner(self) -> Block<SeismicFoundryRpcTransaction, AnyRpcHeader> {
        self.0.into_inner()
    }

    /// Tries to convert inner transactions into a vector of [`AnyRpcTransaction`].
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
    type Header = <SeismicFoundry as Network>::Header;
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
