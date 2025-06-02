//! Seismic overrides to types commonly used in foundry
use std::ops::{Deref, DerefMut};

use alloy_consensus::error::ValueError;
use alloy_network::{AnyHeader, AnyRpcHeader, Network};
use alloy_network_primitives::BlockResponse;
use alloy_rpc_types_eth::{Block, BlockTransactions};
use alloy_serde::WithOtherFields;
use derive_more::From;
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
