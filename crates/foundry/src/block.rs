//! Seismic overrides to types commonly used in foundry
use crate::AnyRpcTransaction;
use alloy_consensus::{error::ValueError};
use alloy_rpc_types_eth::{Block, Header, BlockTransactions};
use alloy_serde::WithOtherFields;
use derive_more::From;
use alloy_network_primitives::{BlockResponse};
use serde::{Deserialize, Serialize};
use alloy_network::Network;

/// Seismic RPC block with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct AnyRpcBlock(pub WithOtherFields<Block<AnyRpcTransaction, Header>>);

impl AnyRpcBlock {
    /// Create a new [`AnyRpcBlock`].
    pub const fn new(inner: WithOtherFields<Block<AnyRpcTransaction, Header>>) -> Self {
        Self(inner)
    }

    /// Consumes the type and returns the wrapped rpc block.
    pub fn into_inner(self) -> Block<AnyRpcTransaction, Header> {
        self.0.into_inner()
    }

    /// Tries to convert inner transactions into a vector of [`AnyRpcTransaction`].
    ///
    /// Returns an error if the block contains only transaction hashes or if it is an uncle block.
    pub fn try_into_transactions(
        self,
    ) -> Result<Vec<AnyRpcTransaction>, ValueError<BlockTransactions<AnyRpcTransaction>>> {
        self.0.inner.try_into_transactions()
    }

    /// Consumes the type and returns an iterator over the transactions in this block
    pub fn into_transactions_iter(self) -> impl Iterator<Item = AnyRpcTransaction> {
        self.into_inner().transactions.into_transactions()
    }
}

impl BlockResponse for AnyRpcBlock {
    type Header = <seismic_alloy_network::SeismicFoundry as Network>::Header;
    type Transaction = <seismic_alloy_network::SeismicFoundry as Network>::TransactionResponse;

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
