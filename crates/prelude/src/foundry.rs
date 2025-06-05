//! Aliases to drop into foundry so we don't have to rename all the types
use alloy_network::{TransactionBuilder, TransactionBuilder4844, TransactionBuilder7702};
use alloy_serde::WithOtherFields;

pub use seismic_alloy_consensus::{
    SeismicReceiptEnvelope as AnyReceiptEnvelope, SeismicTxEnvelope as TxEnvelope, TxSeismic, Decodable712
};
pub use seismic_alloy_network::foundry::{
    block::{
        SeismicFoundryRpcBlock as AnyRpcBlock, SeismicFoundrySimBlock as SimBlock,
        SeismicFoundrySimulatePayload as SimulatePayload,
    },
    builder::seismic_foundry_tx_builder as tx_builder,
    envelope::SeismicFoundryTxEnvelope as AnyTxEnvelope,
    tx_request::{
        SeismicFoundryRpcTransaction as AnyRpcTransaction,
        SeismicFoundryTransactionRequest as AnyTransactionRequest,
        SeismicTransaction as RpcTransaction,
    },
    typed_tx::SeismicFoundryTypedTransaction as AnyTypedTransaction,
    SeismicFoundry as AnyNetwork,
};
pub use seismic_alloy_rpc_types::{
    SeismicTransactionReceipt as TransactionReceipt,
    SeismicTransactionRequest as TransactionRequest,
};

/// A transaction receipt with the SeismicReceiptEnvelope wrapped in a WithOtherFields
pub type AnyTransactionReceipt = WithOtherFields<TransactionReceipt>;
