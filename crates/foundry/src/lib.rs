//! Seismic overrides to types commonly used in foundry
//! We do this to reduce future merge conflicts
mod block;
mod transaction;

pub use seismic_alloy_network::Seismic as AnyNetwork;
pub use seismic_alloy_rpc_types::SeismicTransactionRequest as TransactionRequest;

pub use block::AnyRpcBlock;
pub use transaction::{AnyRpcTransaction, RpcTransaction};
