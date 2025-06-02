//! Aliases to drop into foundry so we don't have to rename all the types
pub use seismic_alloy_network::foundry::{
    block::SeismicFoundryRpcBlock as AnyRpcBlock,
    envelope::SeismicFoundryTxEnvelope as AnyTxEnvelope,
    tx_request::{
        SeismicFoundryRpcTransaction as AnyRpcTransaction,
        SeismicFoundryTransactionRequest as AnyTransactionRequest,
    },
    typed_tx::SeismicFoundryTypedTransaction as AnyTypedTransaction,
    SeismicFoundry as AnyNetwork,
};

// pub use seismic_alloy_network::foundry::{
//     block::SeismicFoundryRpcBlock,
//     envelope::SeismicFoundryTxEnvelope,
//     tx_request::{SeismicFoundryRpcTransaction, SeismicFoundryTransactionRequest},
//     typed_tx::SeismicFoundryTypedTransaction,
//     SeismicFoundry,
// };

pub use seismic_alloy_rpc_types::SeismicTransactionRequest as TransactionRequest;
