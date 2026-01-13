//! Transaction builder for the SeismicFoundry network
use alloy_network::{TransactionBuilder, TransactionBuilder4844, TransactionBuilder7702};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

use crate::foundry::SeismicFoundry;

/// Transaction builder for SeismicFoundry
pub fn seismic_foundry_tx_builder() -> impl TransactionBuilder<SeismicFoundry>
       + TransactionBuilder4844
       + TransactionBuilder7702
       + Into<SeismicTransactionRequest>
       + SeismicTransactionBuilderExt {
    SeismicTransactionRequest::default()
}

/// Extension trait for seismic transaction builders
pub trait SeismicTransactionBuilderExt: Into<SeismicTransactionRequest> + Sized {
    /// Mark this transaction as seismic. Convenience method that converts to
    /// SeismicTransactionRequest and marks it as seismic in one call.
    fn seismic(self) -> SeismicTransactionRequest {
        let tx: SeismicTransactionRequest = self.into();
        tx.seismic()
    }
}

// Implement for SeismicTransactionRequest itself
impl SeismicTransactionBuilderExt for SeismicTransactionRequest {}
