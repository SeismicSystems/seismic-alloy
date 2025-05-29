//! Seismic overrides to types commonly used in foundry
//! We do this to reduce future merge conflicts
use alloy_rpc_types_eth::{Block, Header, Transaction};
use alloy_serde::WithOtherFields;
use derive_more::From;
use serde::{Deserialize, Serialize};

pub use seismic_alloy_rpc_types::SeismicTransactionRequest as TransactionRequest;

/// Seismic RPC transaction
pub type RpcTransaction = Transaction<seismic_alloy_consensus::SeismicTxEnvelope>;

/// Seismic RPC transaction with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct AnyRpcTransaction(pub alloy_serde::WithOtherFields<RpcTransaction>);

/// Seismic RPC block with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct AnyRpcBlock(pub WithOtherFields<Block<RpcTransaction, Header>>);
