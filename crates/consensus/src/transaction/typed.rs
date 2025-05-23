//! Typed transaction types and utilities
use super::InputDecryptionElements;
use crate::InputDecryptionElementsError;
use crate::TxSeismicElements;
use crate::{SeismicTxEnvelope, SeismicTxType, TxSeismic};
use alloy_consensus::{
    transaction::RlpEcdsaEncodableTx, SignableTransaction, Transaction, TxEip1559, TxEip2930,
    TxEip7702, TxLegacy, Typed2718,
};
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{
    bytes::BufMut, Address, Bytes, PrimitiveSignature as Signature, TxHash, TxKind, B256,
};

/// The TypedTransaction enum represents all Ethereum transaction request types, modified for the OP
/// Stack.
///
/// Its variants correspond to specific allowed transactions:
/// 1. Legacy (pre-EIP2718) [`TxLegacy`]
/// 2. EIP2930 (state access lists) [`TxEip2930`]
/// 3. EIP1559 [`TxEip1559`]
/// 4. Seismic [`TxSeismic`]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        from = "serde_from::MaybeTaggedTypedTransaction",
        into = "serde_from::TaggedTypedTransaction"
    )
)]
pub enum SeismicTypedTransaction {
    /// Legacy transaction
    Legacy(TxLegacy),
    /// EIP-2930 transaction
    Eip2930(TxEip2930),
    /// EIP-1559 transaction
    Eip1559(TxEip1559),
    /// EIP-7702 transaction
    Eip7702(TxEip7702),
    /// Seismic deposit transaction
    Seismic(TxSeismic),
}

impl From<TxLegacy> for SeismicTypedTransaction {
    fn from(tx: TxLegacy) -> Self {
        Self::Legacy(tx)
    }
}

impl From<TxEip2930> for SeismicTypedTransaction {
    fn from(tx: TxEip2930) -> Self {
        Self::Eip2930(tx)
    }
}

impl From<TxEip1559> for SeismicTypedTransaction {
    fn from(tx: TxEip1559) -> Self {
        Self::Eip1559(tx)
    }
}

impl From<TxEip7702> for SeismicTypedTransaction {
    fn from(tx: TxEip7702) -> Self {
        Self::Eip7702(tx)
    }
}

impl From<TxSeismic> for SeismicTypedTransaction {
    fn from(tx: TxSeismic) -> Self {
        Self::Seismic(tx)
    }
}

impl From<SeismicTxEnvelope> for SeismicTypedTransaction {
    fn from(envelope: SeismicTxEnvelope) -> Self {
        match envelope {
            SeismicTxEnvelope::Legacy(tx) => Self::Legacy(tx.strip_signature()),
            SeismicTxEnvelope::Eip2930(tx) => Self::Eip2930(tx.strip_signature()),
            SeismicTxEnvelope::Eip1559(tx) => Self::Eip1559(tx.strip_signature()),
            SeismicTxEnvelope::Eip7702(tx) => Self::Eip7702(tx.strip_signature()),
            SeismicTxEnvelope::Seismic(tx) => Self::Seismic(tx.strip_signature()),
        }
    }
}

impl SeismicTypedTransaction {
    /// Return the [`SeismicTxType`] of the inner txn.
    pub const fn tx_type(&self) -> SeismicTxType {
        match self {
            Self::Legacy(_) => SeismicTxType::Legacy,
            Self::Eip2930(_) => SeismicTxType::Eip2930,
            Self::Eip1559(_) => SeismicTxType::Eip1559,
            Self::Eip7702(_) => SeismicTxType::Eip7702,
            Self::Seismic(_) => SeismicTxType::Seismic,
        }
    }

    /// Calculates the signing hash for the transaction.
    ///
    /// Returns `None` if the tx is a deposit transaction.
    pub fn signature_hash(&self) -> B256 {
        match self {
            Self::Legacy(tx) => tx.signature_hash(),
            Self::Eip2930(tx) => tx.signature_hash(),
            Self::Eip1559(tx) => tx.signature_hash(),
            Self::Eip7702(tx) => tx.signature_hash(),
            Self::Seismic(tx) => tx.signature_hash(),
        }
    }

    /// Return the inner legacy transaction if it exists.
    pub const fn legacy(&self) -> Option<&TxLegacy> {
        match self {
            Self::Legacy(tx) => Some(tx),
            _ => None,
        }
    }

    /// Return the inner EIP-2930 transaction if it exists.
    pub const fn eip2930(&self) -> Option<&TxEip2930> {
        match self {
            Self::Eip2930(tx) => Some(tx),
            _ => None,
        }
    }

    /// Return the inner EIP-1559 transaction if it exists.
    pub const fn eip1559(&self) -> Option<&TxEip1559> {
        match self {
            Self::Eip1559(tx) => Some(tx),
            _ => None,
        }
    }

    /// Return the inner deposit transaction if it exists.
    pub const fn deposit(&self) -> Option<&TxSeismic> {
        match self {
            Self::Seismic(tx) => Some(tx),
            _ => None,
        }
    }

    /// Returns `true` if transaction is deposit transaction.
    pub const fn is_deposit(&self) -> bool {
        matches!(self, Self::Seismic(_))
    }
}

impl Typed2718 for SeismicTypedTransaction {
    fn ty(&self) -> u8 {
        match self {
            Self::Legacy(_) => SeismicTxType::Legacy as u8,
            Self::Eip2930(_) => SeismicTxType::Eip2930 as u8,
            Self::Eip1559(_) => SeismicTxType::Eip1559 as u8,
            Self::Eip7702(_) => SeismicTxType::Eip7702 as u8,
            Self::Seismic(_) => SeismicTxType::Seismic as u8,
        }
    }
}

impl Transaction for SeismicTypedTransaction {
    fn chain_id(&self) -> Option<alloy_primitives::ChainId> {
        match self {
            Self::Legacy(tx) => tx.chain_id(),
            Self::Eip2930(tx) => tx.chain_id(),
            Self::Eip1559(tx) => tx.chain_id(),
            Self::Eip7702(tx) => tx.chain_id(),
            Self::Seismic(tx) => tx.chain_id(),
        }
    }

    fn nonce(&self) -> u64 {
        match self {
            Self::Legacy(tx) => tx.nonce(),
            Self::Eip2930(tx) => tx.nonce(),
            Self::Eip1559(tx) => tx.nonce(),
            Self::Eip7702(tx) => tx.nonce(),
            Self::Seismic(tx) => tx.nonce(),
        }
    }

    fn gas_limit(&self) -> u64 {
        match self {
            Self::Legacy(tx) => tx.gas_limit(),
            Self::Eip2930(tx) => tx.gas_limit(),
            Self::Eip1559(tx) => tx.gas_limit(),
            Self::Eip7702(tx) => tx.gas_limit(),
            Self::Seismic(tx) => tx.gas_limit(),
        }
    }

    fn gas_price(&self) -> Option<u128> {
        match self {
            Self::Legacy(tx) => tx.gas_price(),
            Self::Eip2930(tx) => tx.gas_price(),
            Self::Eip1559(tx) => tx.gas_price(),
            Self::Eip7702(tx) => tx.gas_price(),
            Self::Seismic(tx) => tx.gas_price(),
        }
    }

    fn max_fee_per_gas(&self) -> u128 {
        match self {
            Self::Legacy(tx) => tx.max_fee_per_gas(),
            Self::Eip2930(tx) => tx.max_fee_per_gas(),
            Self::Eip1559(tx) => tx.max_fee_per_gas(),
            Self::Eip7702(tx) => tx.max_fee_per_gas(),
            Self::Seismic(tx) => tx.max_fee_per_gas(),
        }
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        match self {
            Self::Legacy(tx) => tx.max_priority_fee_per_gas(),
            Self::Eip2930(tx) => tx.max_priority_fee_per_gas(),
            Self::Eip1559(tx) => tx.max_priority_fee_per_gas(),
            Self::Eip7702(tx) => tx.max_priority_fee_per_gas(),
            Self::Seismic(tx) => tx.max_priority_fee_per_gas(),
        }
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        match self {
            Self::Legacy(tx) => tx.max_fee_per_blob_gas(),
            Self::Eip2930(tx) => tx.max_fee_per_blob_gas(),
            Self::Eip1559(tx) => tx.max_fee_per_blob_gas(),
            Self::Eip7702(tx) => tx.max_fee_per_blob_gas(),
            Self::Seismic(tx) => tx.max_fee_per_blob_gas(),
        }
    }

    fn priority_fee_or_price(&self) -> u128 {
        match self {
            Self::Legacy(tx) => tx.priority_fee_or_price(),
            Self::Eip2930(tx) => tx.priority_fee_or_price(),
            Self::Eip1559(tx) => tx.priority_fee_or_price(),
            Self::Eip7702(tx) => tx.priority_fee_or_price(),
            Self::Seismic(tx) => tx.priority_fee_or_price(),
        }
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        match self {
            Self::Legacy(tx) => tx.effective_gas_price(base_fee),
            Self::Eip2930(tx) => tx.effective_gas_price(base_fee),
            Self::Eip1559(tx) => tx.effective_gas_price(base_fee),
            Self::Eip7702(tx) => tx.effective_gas_price(base_fee),
            Self::Seismic(tx) => tx.effective_gas_price(base_fee),
        }
    }

    fn is_dynamic_fee(&self) -> bool {
        match self {
            Self::Legacy(tx) => tx.is_dynamic_fee(),
            Self::Eip2930(tx) => tx.is_dynamic_fee(),
            Self::Eip1559(tx) => tx.is_dynamic_fee(),
            Self::Eip7702(tx) => tx.is_dynamic_fee(),
            Self::Seismic(tx) => tx.is_dynamic_fee(),
        }
    }

    fn kind(&self) -> TxKind {
        match self {
            Self::Legacy(tx) => tx.kind(),
            Self::Eip2930(tx) => tx.kind(),
            Self::Eip1559(tx) => tx.kind(),
            Self::Eip7702(tx) => tx.kind(),
            Self::Seismic(tx) => tx.kind(),
        }
    }

    fn is_create(&self) -> bool {
        match self {
            Self::Legacy(tx) => tx.is_create(),
            Self::Eip2930(tx) => tx.is_create(),
            Self::Eip1559(tx) => tx.is_create(),
            Self::Eip7702(tx) => tx.is_create(),
            Self::Seismic(tx) => tx.is_create(),
        }
    }

    fn to(&self) -> Option<Address> {
        match self {
            Self::Legacy(tx) => tx.to(),
            Self::Eip2930(tx) => tx.to(),
            Self::Eip1559(tx) => tx.to(),
            Self::Eip7702(tx) => tx.to(),
            Self::Seismic(tx) => tx.to(),
        }
    }

    fn value(&self) -> alloy_primitives::U256 {
        match self {
            Self::Legacy(tx) => tx.value(),
            Self::Eip2930(tx) => tx.value(),
            Self::Eip1559(tx) => tx.value(),
            Self::Eip7702(tx) => tx.value(),
            Self::Seismic(tx) => tx.value(),
        }
    }

    fn input(&self) -> &Bytes {
        match self {
            Self::Legacy(tx) => tx.input(),
            Self::Eip2930(tx) => tx.input(),
            Self::Eip1559(tx) => tx.input(),
            Self::Eip7702(tx) => tx.input(),
            Self::Seismic(tx) => tx.input(),
        }
    }

    fn access_list(&self) -> Option<&AccessList> {
        match self {
            Self::Legacy(tx) => tx.access_list(),
            Self::Eip2930(tx) => tx.access_list(),
            Self::Eip1559(tx) => tx.access_list(),
            Self::Eip7702(tx) => tx.access_list(),
            Self::Seismic(tx) => tx.access_list(),
        }
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        match self {
            Self::Legacy(tx) => tx.blob_versioned_hashes(),
            Self::Eip2930(tx) => tx.blob_versioned_hashes(),
            Self::Eip1559(tx) => tx.blob_versioned_hashes(),
            Self::Eip7702(tx) => tx.blob_versioned_hashes(),
            Self::Seismic(tx) => tx.blob_versioned_hashes(),
        }
    }

    fn authorization_list(&self) -> Option<&[alloy_eips::eip7702::SignedAuthorization]> {
        match self {
            Self::Legacy(tx) => tx.authorization_list(),
            Self::Eip2930(tx) => tx.authorization_list(),
            Self::Eip1559(tx) => tx.authorization_list(),
            Self::Eip7702(tx) => tx.authorization_list(),
            Self::Seismic(tx) => tx.authorization_list(),
        }
    }
}

impl InputDecryptionElements for SeismicTypedTransaction {
    fn get_decryption_elements(&self) -> Result<TxSeismicElements, InputDecryptionElementsError> {
        match self {
            Self::Legacy(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Legacy".to_string(),
            )),
            Self::Eip2930(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Eip2930".to_string(),
            )),
            Self::Eip1559(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Eip1559".to_string(),
            )),
            Self::Eip7702(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Eip7702".to_string(),
            )),
            Self::Seismic(tx) => tx.get_decryption_elements(),
        }
    }

    fn set_input(&mut self, data: Bytes) -> Result<(), InputDecryptionElementsError> {
        match self {
            Self::Legacy(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Legacy".to_string(),
            )),
            Self::Eip2930(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Eip2930".to_string(),
            )),
            Self::Eip1559(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Eip1559".to_string(),
            )),
            Self::Eip7702(_) => Err(InputDecryptionElementsError::UnsupportedTxType(
                "SeismicTypedTransaction::Eip7702".to_string(),
            )),
            Self::Seismic(tx) => tx.set_input(data),
        }
    }
}

impl RlpEcdsaEncodableTx for SeismicTypedTransaction {
    fn rlp_encoded_fields_length(&self) -> usize {
        match self {
            Self::Legacy(tx) => tx.rlp_encoded_fields_length(),
            Self::Eip2930(tx) => tx.rlp_encoded_fields_length(),
            Self::Eip1559(tx) => tx.rlp_encoded_fields_length(),
            Self::Eip7702(tx) => tx.rlp_encoded_fields_length(),
            Self::Seismic(tx) => tx.rlp_encoded_fields_length(),
        }
    }

    fn rlp_encode_fields(&self, out: &mut dyn alloy_rlp::BufMut) {
        match self {
            Self::Legacy(tx) => tx.rlp_encode_fields(out),
            Self::Eip2930(tx) => tx.rlp_encode_fields(out),
            Self::Eip1559(tx) => tx.rlp_encode_fields(out),
            Self::Eip7702(tx) => tx.rlp_encode_fields(out),
            Self::Seismic(tx) => tx.rlp_encode_fields(out),
        }
    }

    fn eip2718_encode_with_type(&self, signature: &Signature, _ty: u8, out: &mut dyn BufMut) {
        match self {
            Self::Legacy(tx) => tx.eip2718_encode_with_type(signature, tx.ty(), out),
            Self::Eip2930(tx) => tx.eip2718_encode_with_type(signature, tx.ty(), out),
            Self::Eip1559(tx) => tx.eip2718_encode_with_type(signature, tx.ty(), out),
            Self::Eip7702(tx) => tx.eip2718_encode_with_type(signature, tx.ty(), out),
            Self::Seismic(tx) => tx.eip2718_encode_with_type(signature, tx.ty(), out),
        }
    }

    fn eip2718_encode(&self, signature: &Signature, out: &mut dyn BufMut) {
        match self {
            Self::Legacy(tx) => tx.eip2718_encode(signature, out),
            Self::Eip2930(tx) => tx.eip2718_encode(signature, out),
            Self::Eip1559(tx) => tx.eip2718_encode(signature, out),
            Self::Eip7702(tx) => tx.eip2718_encode(signature, out),
            Self::Seismic(tx) => tx.eip2718_encode(signature, out),
        }
    }

    fn network_encode_with_type(&self, signature: &Signature, _ty: u8, out: &mut dyn BufMut) {
        match self {
            Self::Legacy(tx) => tx.network_encode_with_type(signature, tx.ty(), out),
            Self::Eip2930(tx) => tx.network_encode_with_type(signature, tx.ty(), out),
            Self::Eip1559(tx) => tx.network_encode_with_type(signature, tx.ty(), out),
            Self::Eip7702(tx) => tx.network_encode_with_type(signature, tx.ty(), out),
            Self::Seismic(tx) => tx.network_encode_with_type(signature, tx.ty(), out),
        }
    }

    fn network_encode(&self, signature: &Signature, out: &mut dyn BufMut) {
        match self {
            Self::Legacy(tx) => tx.network_encode(signature, out),
            Self::Eip2930(tx) => tx.network_encode(signature, out),
            Self::Eip1559(tx) => tx.network_encode(signature, out),
            Self::Eip7702(tx) => tx.network_encode(signature, out),
            Self::Seismic(tx) => tx.network_encode(signature, out),
        }
    }

    fn tx_hash_with_type(&self, signature: &Signature, _ty: u8) -> TxHash {
        match self {
            Self::Legacy(tx) => tx.tx_hash_with_type(signature, tx.ty()),
            Self::Eip2930(tx) => tx.tx_hash_with_type(signature, tx.ty()),
            Self::Eip1559(tx) => tx.tx_hash_with_type(signature, tx.ty()),
            Self::Eip7702(tx) => tx.tx_hash_with_type(signature, tx.ty()),
            Self::Seismic(tx) => tx.tx_hash_with_type(signature, tx.ty()),
        }
    }

    fn tx_hash(&self, signature: &Signature) -> TxHash {
        match self {
            Self::Legacy(tx) => tx.tx_hash(signature),
            Self::Eip2930(tx) => tx.tx_hash(signature),
            Self::Eip1559(tx) => tx.tx_hash(signature),
            Self::Eip7702(tx) => tx.tx_hash(signature),
            Self::Seismic(tx) => tx.tx_hash(signature),
        }
    }
}
#[cfg(feature = "serde")]
mod serde_from {
    //! NB: Why do we need this?
    //!
    //! Because the tag may be missing, we need an abstraction over tagged (with //! type) and
    //! untagged (always legacy). This is [`MaybeTaggedTypedTransaction`].
    //!
    //! The tagged variant is [`TaggedTypedTransaction`], which always has a
    //! type tag.
    //!
    //! We serialize via [`TaggedTypedTransaction`] and deserialize via
    //! [`MaybeTaggedTypedTransaction`].
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    #[serde(untagged)]
    pub(crate) enum MaybeTaggedTypedTransaction {
        Tagged(TaggedTypedTransaction),
        Untagged(TxLegacy),
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "type")]
    pub(crate) enum TaggedTypedTransaction {
        /// Legacy transaction
        #[serde(rename = "0x00", alias = "0x0")]
        Legacy(TxLegacy),
        /// EIP-2930 transaction
        #[serde(rename = "0x01", alias = "0x1")]
        Eip2930(TxEip2930),
        /// EIP-1559 transaction
        #[serde(rename = "0x02", alias = "0x2")]
        Eip1559(TxEip1559),
        /// EIP-7702 transaction
        #[serde(rename = "0x04", alias = "0x4")]
        Eip7702(TxEip7702),
        /// Seismic transaction
        #[serde(rename = "0x4A", alias = "0x4A")]
        Seismic(TxSeismic),
    }

    impl From<MaybeTaggedTypedTransaction> for SeismicTypedTransaction {
        fn from(value: MaybeTaggedTypedTransaction) -> Self {
            match value {
                MaybeTaggedTypedTransaction::Tagged(tagged) => tagged.into(),
                MaybeTaggedTypedTransaction::Untagged(tx) => Self::Legacy(tx),
            }
        }
    }

    impl From<TaggedTypedTransaction> for SeismicTypedTransaction {
        fn from(value: TaggedTypedTransaction) -> Self {
            match value {
                TaggedTypedTransaction::Legacy(signed) => Self::Legacy(signed),
                TaggedTypedTransaction::Eip2930(signed) => Self::Eip2930(signed),
                TaggedTypedTransaction::Eip1559(signed) => Self::Eip1559(signed),
                TaggedTypedTransaction::Eip7702(signed) => Self::Eip7702(signed),
                TaggedTypedTransaction::Seismic(tx) => Self::Seismic(tx),
            }
        }
    }

    impl From<SeismicTypedTransaction> for TaggedTypedTransaction {
        fn from(value: SeismicTypedTransaction) -> Self {
            match value {
                SeismicTypedTransaction::Legacy(signed) => Self::Legacy(signed),
                SeismicTypedTransaction::Eip2930(signed) => Self::Eip2930(signed),
                SeismicTypedTransaction::Eip1559(signed) => Self::Eip1559(signed),
                SeismicTypedTransaction::Eip7702(signed) => Self::Eip7702(signed),
                SeismicTypedTransaction::Seismic(tx) => Self::Seismic(tx),
            }
        }
    }
}
