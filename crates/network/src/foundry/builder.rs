//! Transaction builder for the SeismicFoundry network
use alloy_network::{TransactionBuilder, TransactionBuilder4844, TransactionBuilder7702};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

use crate::foundry::SeismicFoundry;

/// Transaction builder for SeismicFoundry
pub fn seismic_foundry_tx_builder() -> impl TransactionBuilder<SeismicFoundry>
       + TransactionBuilder4844
       + TransactionBuilder7702
       + Into<SeismicTransactionRequest> {
    SeismicTransactionRequest::default()
}
