//! Canonical public payment metadata for Seismic transactions.
use alloy_primitives::{Address, Bytes};
use alloy_rlp::{BufMut, Decodable, Encodable, Header};

/// Signed gas-payment choice. Standard Ethereum transactions always use [`Self::Auto`].
///
/// The mandatory RLP field is a list `[kind, token]`: kind 0/1 requires an empty
/// byte string, while kind 2 requires exactly 20 bytes identifying a nonzero token.
/// JSON is explicitly tagged: `{"type":"auto"}`, `{"type":"native"}`, or
/// `{"type":"token","token":"0x..."}`. This is not Rust enum wire serialization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GasPayment {
    /// Native first, then the first eligible registry entry in insertion order.
    #[default]
    Auto,
    /// Native currency only, without token fallback.
    Native,
    /// This registered token only, without native or other-token fallback.
    Token(Address),
}

/// Invalid selector representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid gas payment selector")]
pub struct InvalidGasPayment;

impl GasPayment {
    /// Canonical numeric tag used by RLP and EIP-712.
    pub const fn kind(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Native => 1,
            Self::Token(_) => 2,
        }
    }

    /// Selected token, absent for automatic or native selection.
    pub const fn token(self) -> Option<Address> {
        match self {
            Self::Token(token) => Some(token),
            _ => None,
        }
    }

    /// Validate a programmatically constructed selector before signing/admission.
    pub const fn validate(self) -> Result<(), InvalidGasPayment> {
        match self {
            Self::Token(Address::ZERO) => Err(InvalidGasPayment),
            _ => Ok(()),
        }
    }

    /// Decode canonical parts; automatic/native selectors cannot carry a token.
    pub fn from_parts(kind: u8, token: Option<Address>) -> Result<Self, InvalidGasPayment> {
        match (kind, token) {
            (0, None) => Ok(Self::Auto),
            (1, None) => Ok(Self::Native),
            (2, Some(token)) if token != Address::ZERO => Ok(Self::Token(token)),
            _ => Err(InvalidGasPayment),
        }
    }
}

impl Encodable for GasPayment {
    fn encode(&self, out: &mut dyn BufMut) {
        let token = self.token();
        let bytes = token.as_ref().map_or(&[][..], |address| address.as_slice());
        Header { list: true, payload_length: self.kind().length() + bytes.length() }.encode(out);
        self.kind().encode(out);
        bytes.encode(out);
    }

    fn length(&self) -> usize {
        let token = self.token();
        let bytes = token.as_ref().map_or(&[][..], |address| address.as_slice());
        let header = Header { list: true, payload_length: self.kind().length() + bytes.length() };
        header.length() + header.payload_length
    }
}

impl Decodable for GasPayment {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        if buf.len() < header.payload_length {
            return Err(alloy_rlp::Error::InputTooShort);
        }
        let (mut fields, rest) = buf.split_at(header.payload_length);
        let kind = u8::decode(&mut fields)?;
        let token = Bytes::decode(&mut fields)?;
        if !fields.is_empty() {
            return Err(alloy_rlp::Error::Custom("extra gas payment fields"));
        }
        let token = match token.len() {
            0 => None,
            20 => Some(Address::from_slice(&token)),
            _ => return Err(alloy_rlp::Error::Custom("invalid gas payment token length")),
        };
        let payment = Self::from_parts(kind, token)
            .map_err(|_| alloy_rlp::Error::Custom("invalid gas payment selector"))?;
        *buf = rest;
        Ok(payment)
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use super::{GasPayment, InvalidGasPayment};
    use alloy_primitives::Address;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
    enum JsonPayment {
        Auto {},
        Native {},
        Token { token: Address },
    }

    impl Serialize for GasPayment {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.validate().map_err(serde::ser::Error::custom)?;
            if !serializer.is_human_readable() {
                return (self.kind(), self.token()).serialize(serializer);
            }
            match *self {
                Self::Auto => JsonPayment::Auto {},
                Self::Native => JsonPayment::Native {},
                Self::Token(token) => JsonPayment::Token { token },
            }
            .serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for GasPayment {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let result: Result<Self, InvalidGasPayment> = if deserializer.is_human_readable() {
                let payment = match JsonPayment::deserialize(deserializer)? {
                    JsonPayment::Auto {} => Self::Auto,
                    JsonPayment::Native {} => Self::Native,
                    JsonPayment::Token { token } => Self::Token(token),
                };
                payment.validate().map(|()| payment)
            } else {
                let (kind, token) = <(u8, Option<Address>)>::deserialize(deserializer)?;
                Self::from_parts(kind, token)
            };
            result.map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(any(test, feature = "arbitrary"))]
impl<'a> arbitrary::Arbitrary<'a> for GasPayment {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(match u.int_in_range(0..=2)? {
            0 => Self::Auto,
            1 => Self::Native,
            _ => {
                let token = Address::arbitrary(u)?;
                if token == Address::ZERO {
                    Self::Auto
                } else {
                    Self::Token(token)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, hex};

    #[test]
    fn canonical_rlp_vectors() {
        let vectors = [
            (GasPayment::Auto, hex!("c28080").to_vec()),
            (GasPayment::Native, hex!("c20180").to_vec()),
            (
                GasPayment::Token(address!("1111111111111111111111111111111111111111")),
                hex!("d602941111111111111111111111111111111111111111").to_vec(),
            ),
        ];
        for (payment, expected) in vectors {
            let encoded = alloy_rlp::encode(payment);
            assert_eq!(encoded, expected);
            assert_eq!(payment.length(), encoded.len());
            let mut buf = encoded.as_slice();
            assert_eq!(GasPayment::decode(&mut buf).unwrap(), payment);
            assert!(buf.is_empty());
        }
    }

    #[test]
    fn rejects_malformed_and_noncanonical_rlp() {
        for bytes in [
            "80",
            "c0",
            "c180",
            "c20080",
            "c20380",
            "c3028080",
            "c20280",
            "c28001",
            "c3018100",
            "c3810180",
            "d602940000000000000000000000000000000000000000",
            "d600941111111111111111111111111111111111111111",
            "d6029311111111111111111111111111111111111111",
        ] {
            let encoded = hex::decode(bytes).unwrap();
            assert!(GasPayment::decode(&mut encoded.as_slice()).is_err(), "accepted {bytes}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn explicit_json_and_bincode_roundtrips() {
        for payment in
            [GasPayment::Auto, GasPayment::Native, GasPayment::Token(Address::repeat_byte(1))]
        {
            let json = serde_json::to_value(payment).unwrap();
            assert_eq!(serde_json::from_value::<GasPayment>(json).unwrap(), payment);
            let binary = bincode::serialize(&payment).unwrap();
            assert_eq!(bincode::deserialize::<GasPayment>(&binary).unwrap(), payment);
        }
        assert_eq!(
            serde_json::to_value(GasPayment::Auto).unwrap(),
            serde_json::json!({"type":"auto"})
        );
        for json in [
            serde_json::json!({"type":"token","token":Address::ZERO}),
            serde_json::json!({"type":"auto","token":Address::repeat_byte(1)}),
            serde_json::json!({"type":"native","unexpected":true}),
            serde_json::json!({"type":"token"}),
            serde_json::json!({"type":"unknown"}),
            serde_json::json!("auto"),
        ] {
            assert!(serde_json::from_value::<GasPayment>(json.clone()).is_err(), "accepted {json}");
        }
    }
}
