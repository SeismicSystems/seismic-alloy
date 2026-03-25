//! EIP-712 typed data decoding
use alloy_dyn_abi::TypedData;
use alloy_primitives::Signature;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
/// An EIP-712 typed data request with a signature
#[derive(Debug, Clone)]
pub struct TypedDataRequest {
    /// The EIP-712 typed data
    pub data: TypedData,
    /// The signature
    pub signature: Signature,
}

/// [EIP-712] decoding errors.
/// [EIP-712]: https://eips.ethereum.org/EIPS/eip-712
#[derive(Clone, Debug)]
#[non_exhaustive] // NB: non-exhaustive allows us to add a Custom variant later
pub enum Eip712Error {
    /// Error while decoding the typed data.
    DecodeError(String),
    /// Got an unexpected type flag while decoding.
    InvalidType,
}

/// Result type for [EIP-712] decoding.
pub type Eip712Result<T, E = Eip712Error> = core::result::Result<T, E>;

/// Decoding trait for [EIP-712] typed data.
///
/// [EIP-712]: https://eips.ethereum.org/EIPS/eip-712
pub trait Decodable712: Sized {
    /// Decode the typed data from the buffer.
    fn decode_712(buf: &TypedDataRequest) -> Eip712Result<Self>;
}

// This is included so that we can have a trait that includes Decodable712
// that can include EthereumTxEnvelope.
impl<T> Decodable712 for alloy_consensus::EthereumTxEnvelope<T> {
    fn decode_712(_: &TypedDataRequest) -> Eip712Result<Self> {
        Err(Eip712Error::InvalidType)
    }
}
