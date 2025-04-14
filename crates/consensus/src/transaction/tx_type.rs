//! Contains the transaction type identifier for Optimism.

use alloy_consensus::Typed2718;
use alloy_eips::eip2718::Eip2718Error;
use alloy_primitives::{U64, U8};
use alloy_rlp::{BufMut, Decodable, Encodable};
use derive_more::Display;

/// Identifier for an Optimism deposit transaction
pub const SEISMIC_TX_TYPE_ID: u8 = 74; // 0x4A

/// Optimism `TransactionType` flags as specified in EIPs [2718], [1559], and
/// [2930], as well as the [deposit transaction spec][deposit-spec]
///
/// [2718]: https://eips.ethereum.org/EIPS/eip-2718
/// [1559]: https://eips.ethereum.org/EIPS/eip-1559
/// [2930]: https://eips.ethereum.org/EIPS/eip-2930
/// [4844]: https://eips.ethereum.org/EIPS/eip-4844
/// [deposit-spec]: https://specs.optimism.io/protocol/deposits.html
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, Default, PartialEq, PartialOrd, Ord, Hash, Display)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(into = "U8", try_from = "U64"))]
pub enum SeismicTxType {
    /// Legacy transaction type.
    #[default]
    #[display("legacy")]
    Legacy = 0,
    /// EIP-2930 transaction type.
    #[display("eip2930")]
    Eip2930 = 1,
    /// EIP-1559 transaction type.
    #[display("eip1559")]
    Eip1559 = 2,
    /// EIP-7702 transaction type.
    #[display("eip7702")]
    Eip7702 = 4,
    /// Seismic transaction type.
    #[display("seismic")]
    Seismic = 74,
}

impl SeismicTxType {
    /// List of all variants.
    pub const ALL: [Self; 5] = [
        Self::Legacy,
        Self::Eip2930,
        Self::Eip1559,
        Self::Eip7702,
        Self::Seismic,
    ];
}

#[cfg(feature = "arbitrary")]
impl arbitrary::Arbitrary<'_> for SeismicTxType {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let i = u.choose_index(Self::ALL.len())?;
        Ok(Self::ALL[i])
    }
}

impl From<SeismicTxType> for U8 {
    fn from(tx_type: SeismicTxType) -> Self {
        Self::from(u8::from(tx_type))
    }
}

impl From<SeismicTxType> for u8 {
    fn from(v: SeismicTxType) -> Self {
        v as Self
    }
}

impl TryFrom<u8> for SeismicTxType {
    type Error = Eip2718Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Legacy,
            1 => Self::Eip2930,
            2 => Self::Eip1559,
            4 => Self::Eip7702,
            74 => Self::Seismic,
            _ => return Err(Eip2718Error::UnexpectedType(value)),
        })
    }
}

impl TryFrom<u64> for SeismicTxType {
    type Error = &'static str;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let err = || "invalid tx type";
        let value: u8 = value.try_into().map_err(|_| err())?;
        Self::try_from(value).map_err(|_| err())
    }
}

impl TryFrom<U64> for SeismicTxType {
    type Error = &'static str;

    fn try_from(value: U64) -> Result<Self, Self::Error> {
        value.to::<u64>().try_into()
    }
}

impl PartialEq<u8> for SeismicTxType {
    fn eq(&self, other: &u8) -> bool {
        (*self as u8) == *other
    }
}

impl PartialEq<SeismicTxType> for u8 {
    fn eq(&self, other: &SeismicTxType) -> bool {
        *self == *other as Self
    }
}

impl Encodable for SeismicTxType {
    fn encode(&self, out: &mut dyn BufMut) {
        (*self as u8).encode(out);
    }

    fn length(&self) -> usize {
        1
    }
}

impl Decodable for SeismicTxType {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let ty = u8::decode(buf)?;

        Self::try_from(ty).map_err(|_| alloy_rlp::Error::Custom("invalid transaction type"))
    }
}

impl Typed2718 for SeismicTxType {
    fn ty(&self) -> u8 {
        (*self).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tx_types() {
        assert_eq!(SeismicTxType::ALL.len(), 5);
        let all = vec![
            SeismicTxType::Legacy,
            SeismicTxType::Eip2930,
            SeismicTxType::Eip1559,
            SeismicTxType::Eip7702,
            SeismicTxType::Seismic,
        ];
        assert_eq!(SeismicTxType::ALL.to_vec(), all);
    }

    #[test]
    fn tx_type_roundtrip() {
        for &tx_type in &SeismicTxType::ALL {
            let mut buf = Vec::new();
            tx_type.encode(&mut buf);
            let decoded = SeismicTxType::decode(&mut &buf[..]).unwrap();
            assert_eq!(tx_type, decoded);
        }
    }
}
