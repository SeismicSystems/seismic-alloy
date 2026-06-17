use alloy_primitives::Bytes;
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
    /// An EIP-712 typed data request with a signature; this variant is a convenience for wallets
    /// that can sign EIP-712 typed data but don't natively produce RLP-encoded `0x4A`
    /// transactions.
    ///
    /// TODO(samlaf): this type/path carries no information that isn't equally expressible as an
    /// RLP-encoded `TxSeismic` with the EIP-712 signature attached, which clients can
    /// submit via the [`Self::Bytes`] variant instead. Reth currently handles this variant
    /// by re-encoding to RLP internally anyways. Clients should migrate to submitting RLP bytes
    /// via `Bytes`; once done, this variant can be removed entirely and the server's re-encode
    /// logic collapses.
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

impl Into<SeismicCallRequest> for WithOtherFields<SeismicTransactionRequest> {
    fn into(self) -> SeismicCallRequest {
        SeismicCallRequest::TransactionRequest(self.inner.into())
    }
}
