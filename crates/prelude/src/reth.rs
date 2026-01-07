//! Aliases to drop into reth so we don't have to rename all the types
// TODO: will this be helpful inside reth?

pub use seismic_alloy_consensus::{
    Decodable712, Eip712Result, InputDecryptionElements, SeismicReceiptEnvelope, SeismicTxEnvelope,
    TxLegacyFields, TxSeismic, TxSeismicElements, TxSeismicMetadata, TypedDataRequest,
    SEISMIC_TX_TYPE_ID,
};
