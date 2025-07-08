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
        unimplemented!("Should not be called becuase Ethereum Reth does not support EIP-712")
    }
}

/// Parse the message out of typed data as a serde json value
#[cfg(feature = "serde")]
pub fn parse_typed_data_message(typed_data: &TypedData) -> Eip712Result<serde_json::Value> {
    let message = serde_json::to_value(&typed_data.message)
        .map_err(|_| Eip712Error::DecodeError("Failed to serialize message".to_string()))?;
    Ok(message)
}

#[cfg(feature = "serde")]
fn parse_u8(message: &serde_json::Value, field: &'static str) -> Eip712Result<u8> {
    let v_u64 = match message.get(field) {
        Some(v) => match v.as_u64() {
            Some(v_u64) => Ok(v_u64),
            None => Err(Eip712Error::DecodeError(format!(
                "Failed to parse '{}' as integer. Received: {:?}",
                field, v
            ))),
        },
        None => Err(Eip712Error::DecodeError(format!("Missing field '{}' in typed data", field))),
    }?;
    let v_u8 = match v_u64 < u64::from(u8::MAX) {
        true => Ok(v_u64 as u8),
        false => {
            Err(Eip712Error::DecodeError(format!("'{}' {} is too large for u8", field, v_u64)))
        }
    }?;
    Ok(v_u8)
}

/// represents what kind of transaction they are sending via typed data
#[derive(PartialEq, Debug)]
pub enum TypedDataTransactionType {
    /// always a seismic transaction
    TxSeismic,
}

#[cfg(feature = "serde")]
impl TypedDataTransactionType {
    /// Parse transaction type out of the typed data
    pub fn parse_type(typed_data: &TypedData) -> Eip712Result<TypedDataTransactionType> {
        let message = parse_typed_data_message(typed_data)?;
        let v_u8 = parse_u8(&message, "messageVersion")?;
        match v_u8 {
            2 => Ok(TypedDataTransactionType::TxSeismic),
            _ => Err(Eip712Error::DecodeError(format!(
                "Invalid 'messageVersion' for typed data transaction: {:?}. Allowed values: (2, 3)",
                v_u8
            ))),
        }
    }
}
