use crate::{transaction::RlpEcdsaTx, SignableTransaction, Signed, Transaction, TxType, Typed2718};
use alloy_dyn_abi::{Eip712Domain, Resolver, TypedData};
use alloy_eips::{eip2930::AccessList, eip7702::SignedAuthorization};
use alloy_primitives::{
    keccak256, Address, Bytes, ChainId, FixedBytes, PrimitiveSignature as Signature, TxKind, B256,
    U256,
};
use alloy_rlp::{BufMut, Decodable, Encodable};
use core::mem;

/// Compressed secp256k1 public key
pub type EncryptionPublicKey = FixedBytes<33>;

/// Basic encrypted transaction type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
#[cfg_attr(any(test, feature = "arbitrary"), derive(arbitrary::Arbitrary))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[doc(alias = "SeismicTransaction", alias = "TransactionSeismic", alias = "SeismicTx")]
pub struct TxSeismic {
    /// encrypted transaction inputted from users
    #[cfg_attr(feature = "serde", serde(with = "alloy_serde::quantity"))]
    pub chain_id: ChainId,
    /// A scalar value equal to the number of transactions sent by the sender; formally Tn.
    #[cfg_attr(feature = "serde", serde(with = "alloy_serde::quantity"))]
    pub nonce: u64,
    /// A scalar value equal to the number of
    /// Wei to be paid per unit of gas for all computation
    /// costs incurred as a result of the execution of this transaction; formally Tp.
    ///
    /// As ethereum circulation is around 120mil eth as of 2022 that is around
    /// 120000000000000000000000000 wei we are safe to use u128 as its max number is:
    /// 340282366920938463463374607431768211455
    #[cfg_attr(feature = "serde", serde(with = "alloy_serde::quantity"))]
    pub gas_price: u128,
    /// A scalar value equal to the maximum
    /// amount of gas that should be used in executing
    /// this transaction. This is paid up-front, before any
    /// computation is done and may not be increased
    /// later; formally Tg.
    #[cfg_attr(
        feature = "serde",
        serde(with = "alloy_serde::quantity", rename = "gas", alias = "gasLimit")
    )]
    pub gas_limit: u64,
    /// The 160-bit address of the message call’s recipient or, for a contract creation
    /// transaction, ∅, used here to denote the only member of B0 ; formally Tt.
    #[cfg_attr(feature = "serde", serde(default))]
    pub to: TxKind,
    /// A scalar value equal to the number of Wei to
    /// be transferred to the message call’s recipient or,
    /// in the case of contract creation, as an endowment
    /// to the newly created account; formally Tv.
    pub value: U256,
    /// The public key we will decrypt to
    #[cfg_attr(feature = "serde", serde(alias = "encryptionPubkey"))]
    pub encryption_pubkey: EncryptionPublicKey,
    /// The EIP712 version of the transaction when the user submitted it using signTypedDataV4.
    /// A value of 0 means the transaction was not signed using EIP712
    #[cfg_attr(feature = "serde", serde(alias = "eip712Version", default))]
    pub eip712_version: u8,
    /// Input has two uses depending if transaction is Create or Call (if `to` field is None or
    /// Some). pub init: An unlimited size byte array specifying the
    /// EVM-code for the account initialisation procedure CREATE,
    /// data: An unlimited size byte array specifying the
    /// input data of the message call, formally Td.
    pub input: Bytes,
}

impl TxSeismic {
    /// numeric type for the transaction
    pub const TX_TYPE: u8 = 0x4A;

    /// Get the transaction type
    #[doc(alias = "transaction_type")]
    pub(crate) const fn tx_type() -> TxType {
        TxType::Seismic
    }

    /// Calculates a heuristic for the in-memory size of the [`TxSeismic`] transaction.
    /// In memory stores the decrypted transaction and the encrypted transaction.
    /// Out of memory stores the encrypted transaction. This is why size and fields_len are
    /// diffenrent.
    #[inline]
    pub fn size(&self) -> usize {
        mem::size_of::<ChainId>() + // chain_id
        mem::size_of::<u64>() + // nonce
        mem::size_of::<u128>() + // gas_price
        mem::size_of::<u64>() + // gas_limit
        mem::size_of::<u128>() + // max_priority_fee_per_gas
        self.to.size() + // to
        mem::size_of::<U256>() + // value
        self.encryption_pubkey.len() + // encryption public key
        mem::size_of::<u8>() + // eip712_version
        self.input.len() // input
    }

    fn eip712_typed_data(&self) -> TypedData {
        let typed_data_json = serde_json::json!({
            "types": {
                "EIP712Domain": [
                  { "name": "name", "type": "string" },
                  { "name": "version", "type": "string" },
                  { "name": "chainId", "type": "uint256" },
                  { "name": "verifyingContract", "type": "address" },
                ],
                "TxSeismic": [
                  { "name": "chainId", "type": "uint64" },
                  { "name": "nonce", "type": "uint64" },
                  { "name": "gasPrice", "type": "uint128" },
                  { "name": "gasLimit", "type": "uint64" },
                  // if blank, we assume it's a create
                  { "name": "to", "type": "address" },
                  { "name": "value", "type": "uint256" },
                  // compressed secp256k1 public key (33 bytes)
                  { "name": "encryptionPubkey", "type": "bytes" },
                  { "name": "eip712Version", "type": "uint8" },
                  { "name": "input", "type": "bytes" },
                ],
            },
            "primaryType": "TxSeismic",
            "domain": {
                "name": "Seismic Transaction",
                "version": self.eip712_version.to_string(),
                "chainId": self.chain_id,
                // no verifying contract since this happens in RPC
                "verifyingContract": "0x0000000000000000000000000000000000000000",
            },
            "message": {
                "chainId": self.chain_id,
                "nonce": self.nonce,
                "gasPrice": self.gas_price,
                "gasLimit": self.gas_limit,
                "to": self.to,
                "value": self.value,
                "input": self.input,
                "encryptionPubkey": self.encryption_pubkey,
                "eip712Version": self.eip712_version,
            }
        });
        serde_json::from_value(typed_data_json).unwrap()
    }

    fn eip712_signature_hash(&self) -> B256 {
        let typed_data = self.eip712_typed_data();
        typed_data.eip712_signing_hash().unwrap()
    }
}

impl RlpEcdsaTx for TxSeismic {
    const DEFAULT_TX_TYPE: u8 = { Self::tx_type() as u8 };

    /// Outputs the length of the transaction's fields, without a RLP header.
    fn rlp_encoded_fields_length(&self) -> usize {
        self.chain_id.length()
            + self.nonce.length()
            + self.gas_price.length()
            + self.gas_limit.length()
            + self.to.length()
            + self.value.length()
            + self.encryption_pubkey.length()
            + self.eip712_version.length()
            + self.input.length()
    }

    /// Encodes only the transaction's fields into the desired buffer, without
    /// a RLP header.
    fn rlp_encode_fields(&self, out: &mut dyn alloy_rlp::BufMut) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.gas_price.encode(out);
        self.gas_limit.encode(out);
        self.to.encode(out);
        self.value.encode(out);
        self.encryption_pubkey.encode(out);
        self.eip712_version.encode(out);
        self.input.encode(out);
    }

    /// Decodes the inner [TxSeismic] fields from RLP bytes.
    ///
    /// NOTE: This assumes a RLP header has already been decoded, and _just_
    /// decodes the following RLP fields in the following order:
    ///
    /// - `chain_id`
    /// - `nonce`
    /// - `max_priority_fee_per_gas`
    /// - `max_fee_per_gas`
    /// - `gas_limit`
    /// - `to`
    /// - `value`
    /// - `data` (`input`)
    /// - `encryption_pubkey`
    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Ok(Self {
            chain_id: Decodable::decode(buf)?,
            nonce: Decodable::decode(buf)?,
            gas_price: Decodable::decode(buf)?,
            gas_limit: Decodable::decode(buf)?,
            to: Decodable::decode(buf)?,
            value: Decodable::decode(buf)?,
            encryption_pubkey: Decodable::decode(buf)?,
            eip712_version: Decodable::decode(buf)?,
            input: Decodable::decode(buf)?,
        })
    }
}

impl Transaction for TxSeismic {
    #[inline]
    fn chain_id(&self) -> Option<ChainId> {
        Some(self.chain_id)
    }

    #[inline]
    fn nonce(&self) -> u64 {
        self.nonce
    }

    #[inline]
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    #[inline]
    fn gas_price(&self) -> Option<u128> {
        Some(self.gas_price)
    }

    #[inline]
    fn max_fee_per_gas(&self) -> u128 {
        self.gas_price
    }

    #[inline]
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        None
    }

    #[inline]
    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        None
    }

    #[inline]
    fn priority_fee_or_price(&self) -> u128 {
        self.gas_price
    }

    fn effective_gas_price(&self, _base_fee: Option<u64>) -> u128 {
        self.gas_price
    }

    #[inline]
    fn is_dynamic_fee(&self) -> bool {
        false
    }

    #[inline]
    fn kind(&self) -> TxKind {
        self.to
    }

    #[inline]
    fn is_create(&self) -> bool {
        self.to.is_create()
    }

    #[inline]
    fn value(&self) -> U256 {
        self.value
    }

    #[inline]
    fn input(&self) -> &Bytes {
        &self.input
    }

    #[inline]
    fn access_list(&self) -> Option<&AccessList> {
        None
    }

    #[inline]
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        None
    }

    #[inline]
    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        None
    }

    #[inline]
    fn encryption_pubkey(&self) -> Option<&FixedBytes<33>> {
        Some(&self.encryption_pubkey)
    }

    #[inline]
    fn eip712_version(&self) -> Option<u8> {
        Some(self.eip712_version)
    }
}

impl Typed2718 for TxSeismic {
    fn ty(&self) -> u8 {
        TxType::Seismic as u8
    }
}

impl SignableTransaction<Signature> for TxSeismic {
    fn set_chain_id(&mut self, chain_id: ChainId) {
        self.chain_id = chain_id;
    }

    fn encode_for_signing(&self, out: &mut dyn alloy_rlp::BufMut) {
        out.put_u8(Self::tx_type() as u8);
        self.encode(out)
    }

    fn payload_len_for_signature(&self) -> usize {
        self.length() + 1
    }

    fn into_signed(self, signature: Signature) -> Signed<Self> {
        let tx_hash = self.tx_hash(&signature);
        Signed::new_unchecked(self, signature, tx_hash)
    }

    fn signature_hash(&self) -> B256 {
        match self.eip712_version {
            0 => keccak256(self.encoded_for_signing()),
            _ => self.eip712_signature_hash(),
        }
    }
}

impl Encodable for TxSeismic {
    fn encode(&self, out: &mut dyn BufMut) {
        self.rlp_encode(out);
    }

    fn length(&self) -> usize {
        self.rlp_encoded_length()
    }
}

impl Decodable for TxSeismic {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::rlp_decode(buf)
    }
}

/// Bincode-compatible [`TxSeismic`] serde implementation.
#[cfg(all(feature = "serde", feature = "serde-bincode-compat"))]
pub(super) mod serde_bincode_compat {
    use alloc::borrow::Cow;
    use alloy_primitives::{Bytes, ChainId, TxKind, U256};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_with::{DeserializeAs, SerializeAs};

    /// Bincode-compatible [`super::TxSeismic`] serde implementation.
    ///
    /// Intended to use with the [`serde_with::serde_as`] macro in the following way:
    /// ```rust
    /// use alloy_consensus::{serde_bincode_compat, TxSeismic};
    /// use serde::{Deserialize, Serialize};
    /// use serde_with::serde_as;
    ///
    /// #[serde_as]
    /// #[derive(Serialize, Deserialize)]
    /// struct Data {
    ///     #[serde_as(as = "serde_bincode_compat::transaction::TxSeismic")]
    ///     header: TxSeismic,
    /// }
    /// ```
    #[derive(Debug, Serialize, Deserialize)]
    pub struct TxSeismic<'a> {
        chain_id: ChainId,
        nonce: u64,
        gas_price: u128,
        gas_limit: u64,
        #[serde(default)]
        to: TxKind,
        value: U256,
        encryption_pubkey: Cow<'a, crate::transaction::EncryptionPublicKey>,
        eip712_version: u8,
        input: Cow<'a, Bytes>,
    }

    impl<'a> From<&'a super::TxSeismic> for TxSeismic<'a> {
        fn from(value: &'a super::TxSeismic) -> Self {
            Self {
                chain_id: value.chain_id,
                nonce: value.nonce,
                gas_price: value.gas_price,
                gas_limit: value.gas_limit,
                to: value.to,
                value: value.value,
                encryption_pubkey: Cow::Borrowed(&value.encryption_pubkey),
                eip712_version: value.eip712_version,
                input: Cow::Borrowed(&value.input),
            }
        }
    }

    impl<'a> From<TxSeismic<'a>> for super::TxSeismic {
        fn from(value: TxSeismic<'a>) -> Self {
            Self {
                chain_id: value.chain_id,
                nonce: value.nonce,
                gas_price: value.gas_price,
                gas_limit: value.gas_limit,
                to: value.to,
                value: value.value,
                encryption_pubkey: value.encryption_pubkey.into_owned(),
                eip712_version: value.eip712_version,
                input: value.input.into_owned(),
            }
        }
    }

    impl SerializeAs<super::TxSeismic> for TxSeismic<'_> {
        fn serialize_as<S>(source: &super::TxSeismic, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            TxSeismic::from(source).serialize(serializer)
        }
    }

    impl<'de> DeserializeAs<'de, super::TxSeismic> for TxSeismic<'de> {
        fn deserialize_as<D>(deserializer: D) -> Result<super::TxSeismic, D::Error>
        where
            D: Deserializer<'de>,
        {
            TxSeismic::deserialize(deserializer).map(Into::into)
        }
    }

    #[cfg(test)]
    mod tests {
        use arbitrary::Arbitrary;
        use rand::Rng;
        use serde::{Deserialize, Serialize};
        use serde_with::serde_as;

        use super::super::{serde_bincode_compat, TxSeismic};

        #[test]
        fn test_tx_legacy_bincode_roundtrip() {
            #[serde_as]
            #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
            struct Data {
                #[serde_as(as = "serde_bincode_compat::TxSeismic")]
                transaction: TxSeismic,
            }

            let mut bytes = [0u8; 1024];
            rand::thread_rng().fill(bytes.as_mut_slice());
            let data = Data {
                transaction: TxSeismic::arbitrary(&mut arbitrary::Unstructured::new(&bytes))
                    .unwrap(),
            };

            let encoded = bincode::serialize(&data).unwrap();
            let decoded: Data = bincode::deserialize(&encoded).unwrap();
            assert_eq!(decoded, data);
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{b256, hex, Address};
    use derive_more::FromStr;

    use super::*;

    #[test]
    fn test_encode_decode_seismic() {
        let hash: B256 = b256!("1ecf0fb8b70b4e94745ac04bd99f07321199fce3a8f58b3bc3f9c9c837e47a73");

        let tx = TxSeismic {
            chain_id: 4u64,
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: Address::from_str("d3e8763675e4c425df46cc3b5c0f6cbdac396046").unwrap().into(),
            value: U256::from(1000000000000000u64),
            encryption_pubkey: hex!("028e76821eb4d77fd30223ca971c49738eb5b5b71eabe93f96b348fdce788ae5a0").into(),
            eip712_version: 0,
            input:  hex!("a22cb4650000000000000000000000005eee75727d804a2b13038928d36f8b188945a57a0000000000000000000000000000000000000000000000000000000000000000").into(),
        };

        let sig = Signature::from_scalars_and_parity(
            b256!("840cfc572845f5786e702984c2a582528cad4b49b2a10b9db1be7fca90058565"),
            b256!("25e7109ceb98168d95b09b18bbf6b685130e0562f233877d492b94eee0c5b6d1"),
            false,
        );

        let mut buf = vec![];
        tx.rlp_encode_signed(&sig, &mut buf);
        let decoded = TxSeismic::rlp_decode_signed(&mut &buf[..]).unwrap();
        let signer = decoded.recover_signer().unwrap();
        assert_eq!(signer, Address::from_str("0xe71a5dd0b0471f425f48ca05376f2251d58af0ea").unwrap());
        assert_eq!(decoded, tx.clone().into_signed(sig));
        assert_eq!(*decoded.hash(), hash);
        assert_eq!(decoded.tx().clone(), tx.clone());
    }

    #[test]
    fn test_eip712_signature_hash() {
        let tx = TxSeismic {
            chain_id: 4u64,
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: Address::from_str("d3e8763675e4c425df46cc3b5c0f6cbdac396046").unwrap().into(),
            value: U256::from(1000000000000000u64),
            encryption_pubkey: hex!("028e76821eb4d77fd30223ca971c49738eb5b5b71eabe93f96b348fdce788ae5a0").into(),
            eip712_version: 1,
            input:  hex!("a22cb4650000000000000000000000005eee75727d804a2b13038928d36f8b188945a57a0000000000000000000000000000000000000000000000000000000000000000").into(),
        };
        let hash = tx.eip712_signature_hash();
        assert_eq!(hash, hex!("261fbcf5298b1f7583525a9e29d6766dfcd97b379915b0b95e98d4face1d9182"));

        let sig = Signature::from_scalars_and_parity(
            b256!("840cfc572845f5786e702984c2a582528cad4b49b2a10b9db1be7fca90058565"),
            b256!("25e7109ceb98168d95b09b18bbf6b685130e0562f233877d492b94eee0c5b6d1"),
            false,
        );
        let mut buf = vec![];
        tx.rlp_encode_signed(&sig, &mut buf);
        let decoded = TxSeismic::rlp_decode_signed(&mut &buf[..]).unwrap();
        let signer = decoded.recover_signer().unwrap();
        assert_eq!(signer, Address::from_str("0x69e069c42cb8a5332276613dfbd4823c0ed8043d").unwrap());

        let hash = tx.tx_hash(&sig);
        assert_eq!(hash, b256!("539439da42159d6f7220ad3e5590a05c2193a99d8b9ba0316b2c6f622f9cf7c6"));
    }
}
