use alloy_primitives::Bytes;
use alloy_rpc_types_eth::TransactionRequest;
use alloy_serde::WithOtherFields;
use seismic_alloy_consensus::TypedDataRequest;

use crate::SeismicTransactionRequest;

/// Either normal raw tx or typed data with signature
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum SeismicRawTxRequest {
    /// A raw seismic tx
    Bytes(Bytes),
    /// An EIP-712 typed data request with a signature
    TypedData(TypedDataRequest),
}

impl Into<SeismicRawTxRequest> for Bytes {
    fn into(self) -> SeismicRawTxRequest {
        SeismicRawTxRequest::Bytes(self)
    }
}

impl Into<SeismicRawTxRequest> for TypedDataRequest {
    fn into(self) -> SeismicRawTxRequest {
        SeismicRawTxRequest::TypedData(self)
    }
}

/// Either a normal ETH call, raw tx, or typed data with signature
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum SeismicCallRequest {
    /// EIP-712 signed typed message with signature
    TypedData(TypedDataRequest),
    /// normal call request
    TransactionRequest(SeismicTransactionRequest),
    /// signed raw seismic tx
    Bytes(Bytes),
}

impl Default for SeismicCallRequest {
    fn default() -> Self {
        SeismicCallRequest::TransactionRequest(SeismicTransactionRequest::default())
    }
}

impl Into<SeismicCallRequest> for TypedDataRequest {
    fn into(self) -> SeismicCallRequest {
        SeismicCallRequest::TypedData(self)
    }
}

impl Into<SeismicCallRequest> for SeismicTransactionRequest {
    fn into(self) -> SeismicCallRequest {
        SeismicCallRequest::TransactionRequest(self)
    }
}

impl Into<SeismicCallRequest> for Bytes {
    fn into(self) -> SeismicCallRequest {
        SeismicCallRequest::Bytes(self)
    }
}
