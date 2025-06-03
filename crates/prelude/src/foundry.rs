//! Aliases to drop into foundry so we don't have to rename all the types
use alloy_serde::WithOtherFields;

pub use seismic_alloy_consensus::SeismicReceiptEnvelope as AnyReceiptEnvelope;
pub use seismic_alloy_consensus::SeismicTxEnvelope as TxEnvelope;
pub use seismic_alloy_network::foundry::{
    block::SeismicFoundryRpcBlock as AnyRpcBlock,
    envelope::SeismicFoundryTxEnvelope as AnyTxEnvelope,
    tx_request::{
        SeismicFoundryRpcTransaction as AnyRpcTransaction,
        SeismicFoundryTransactionRequest as AnyTransactionRequest,
        SeismicTransaction as RpcTransaction,
    },
    typed_tx::SeismicFoundryTypedTransaction as AnyTypedTransaction,
    SeismicFoundry as AnyNetwork,
};
pub use seismic_alloy_rpc_types::SeismicTransactionReceipt as TransactionReceipt;
pub use seismic_alloy_rpc_types::SeismicTransactionRequest as TransactionRequest;

/// A transaction receipt with the SeismicReceiptEnvelope wrapped in a WithOtherFields
pub type AnyTransactionReceipt = WithOtherFields<TransactionReceipt>;

// pub use seismic_alloy_network::foundry::{
//     block::SeismicFoundryRpcBlock,
//     envelope::SeismicFoundryTxEnvelope,
//     tx_request::{SeismicFoundryRpcTransaction, SeismicFoundryTransactionRequest},
//     typed_tx::SeismicFoundryTypedTransaction,
//     SeismicFoundry,
// };
