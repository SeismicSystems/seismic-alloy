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
    /// An EIP-712 typed data request with a signature.
    ///
    /// TODO(deprecate TypedData RPC variant): this variant is a convenience for wallets
    /// that can sign EIP-712 typed data but don't natively produce RLP-encoded `0x4A`
    /// transactions. It carries no information that isn't equally expressible as an
    /// RLP-encoded `TxSeismic` with `message_version = 2` and the EIP-712 signature,
    /// which clients can submit via the [`Self::Bytes`] variant instead. The server
    /// currently handles this variant by re-encoding to RLP internally (see
    /// `send_raw_transaction` in seismic-reth's `ext.rs`) so all signed-tx ingress
    /// funnels through the single `Decodable2718::typed_decode` pipeline.
    ///
    /// Clients should migrate to submitting RLP bytes via `Bytes`. This variant is
    /// planned for removal in a future hard fork, ideally alongside the wire-format
    /// split that replaces `message_version` with a distinct EIP-2718 type byte
    /// (`0x4C`) — see the companion TODOs in seismic-alloy's `envelope.rs` (for
    /// `0x4B` signed-reads) and seismic-reth's `ext.rs` (for `0x4C` EIP-712 writes).
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
