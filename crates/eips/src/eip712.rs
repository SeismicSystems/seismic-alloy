//! EIP-712 typed data decoding

use alloy_dyn_abi::TypedData;
use alloy_primitives::PrimitiveSignature;
use serde::{Deserialize, Serialize};

/// An EIP-712 typed data request with a signature
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TypedDataRequest {
    /// The EIP-712 typed data
    pub data: TypedData,
    /// The signature
    pub signature: PrimitiveSignature,
}

/// [EIP-712] decoding errors.
/// [EIP-712]: https://eips.ethereum.org/EIPS/eip-712
#[derive(Clone, Copy, Debug)]
#[non_exhaustive] // NB: non-exhaustive allows us to add a Custom variant later
pub enum Eip712Error {
    /// Rlp error from [`alloy_rlp`].
    RlpError(alloy_rlp::Error),
    /// Got an unexpected type flag while decoding.
    UnexpectedType(u8),
}

/// Result type for [EIP-712] decoding.
pub type Eip712Result<T, E = Eip712Error> = core::result::Result<T, E>;

/// Decoding trait for [EIP-712] typed data.
///
/// [EIP-712]: https://eips.ethereum.org/EIPS/eip-712
pub trait Decodable712: Sized {
    /// Decode the typed data from the buffer.
    fn decode_712(buf: &mut &TypedDataRequest) -> Eip712Result<Self>;
}
