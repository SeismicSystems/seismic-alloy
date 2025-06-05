//! Transaction builder for the SeismicReth network
use alloy_network::{TransactionBuilder, TransactionBuilder4844, TransactionBuilder7702};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

use crate::SeismicReth;

/// Transaction builder for the SeismicReth network
pub fn seismic_reth_tx_builder() -> impl TransactionBuilder<SeismicReth>
       + TransactionBuilder4844
       + TransactionBuilder7702
       + Into<SeismicTransactionRequest> {
    SeismicTransactionRequest::default()
}
