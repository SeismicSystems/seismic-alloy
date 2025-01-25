#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/alloy-rs/core/main/assets/alloy.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/alloy-rs/core/main/assets/favicon.ico"
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

pub use alloy_serde as serde_helpers;

mod rpc;
pub use rpc::*;

#[cfg(feature = "admin")]
pub use alloy_rpc_types_admin as admin;

#[cfg(feature = "anvil")]
pub use alloy_rpc_types_anvil as anvil;

#[cfg(feature = "beacon")]
pub use alloy_rpc_types_beacon as beacon;

#[cfg(feature = "debug")]
pub use alloy_rpc_types_debug as debug;

#[cfg(feature = "engine")]
pub use alloy_rpc_types_engine as engine;

#[cfg(feature = "eth")]
pub use alloy_rpc_types_eth as eth;
#[cfg(feature = "eth")]
pub use eth::*;

#[cfg(feature = "mev")]
pub use alloy_rpc_types_mev as mev;

#[cfg(feature = "trace")]
pub use alloy_rpc_types_trace as trace;

#[cfg(feature = "txpool")]
pub use alloy_rpc_types_txpool as txpool;

/// Seismic RPC types
pub mod seismic {
    use alloy_dyn_abi::TypedData;
    use alloy_primitives::{Bytes, PrimitiveSignature};
    use crate::TransactionRequest;
    use alloy_serde::WithOtherFields;
    use serde::{Deserialize, Serialize};

    /// An EIP-712 typed data request with a signature
    #[derive(Debug, Deserialize, Serialize, Clone)]
    pub struct TypedDataRequest {
        /// The EIP-712 typed data
        pub data: TypedData,
        /// The signature
        pub signature: PrimitiveSignature,
    }

    /// Either normal raw tx or typed data with signature
    #[derive(Debug, Deserialize, Serialize, Clone)]
    #[serde(untagged)]
    pub enum SeismicRawTxRequest {
        /// A raw seismic tx
        Bytes(Bytes),
        /// An EIP-712 typed data request with a signature
        TypedData(TypedDataRequest),
    }

    /// Either a normal ETH call, raw tx, or typed data with signature
    #[derive(Debug, Deserialize, Serialize, Clone)]
    #[serde(untagged)]
    pub enum SeismicCallRequest {
        /// EIP-712 signed typed message with signature
        TypedData(TypedDataRequest),
        /// normal call request
        TransactionRequest(WithOtherFields<TransactionRequest>),
        /// signed raw seismic tx
        Bytes(Bytes),
    }

    impl Into<SeismicCallRequest> for TypedDataRequest {
        fn into(self) -> SeismicCallRequest {
            SeismicCallRequest::TypedData(self)
        }
    }

    impl Into<SeismicCallRequest> for WithOtherFields<TransactionRequest> {
        fn into(self) -> SeismicCallRequest {
            SeismicCallRequest::TransactionRequest(self)
        }
    }

    impl Into<SeismicCallRequest> for Bytes {
        fn into(self) -> SeismicCallRequest {
            SeismicCallRequest::Bytes(self)
        }
    }
}