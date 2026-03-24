//! Seismic Foundry typed transaction, meant to mimic AnyTypedTransaction
use alloy_consensus::TypedTransaction;
use alloy_network::{AnyTypedTransaction, UnknownTypedTransaction};
use seismic_alloy_consensus::TxSeismic;

use crate::foundry::envelope::SeismicFoundryTxEnvelope;

/// Unsigned transaction type for a catch-all network.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum SeismicFoundryTypedTransaction {
    /// An Ethereum transaction.
    Ethereum(TypedTransaction),
    /// A transaction with unknown type.
    Unknown(UnknownTypedTransaction),
    /// A Seismic transaction.
    Seismic(TxSeismic),
}

impl From<SeismicFoundryTxEnvelope> for SeismicFoundryTypedTransaction {
    fn from(envelope: SeismicFoundryTxEnvelope) -> Self {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = envelope {
            Self::Seismic(tx.strip_signature())
        } else {
            let any_typed: AnyTypedTransaction =
                envelope.to_any_tx_envelope().expect("non-Seismic variant").into();
            match any_typed {
                AnyTypedTransaction::Ethereum(tx) => Self::Ethereum(tx),
                AnyTypedTransaction::Unknown(tx) => Self::Unknown(tx),
            }
        }
    }
}
