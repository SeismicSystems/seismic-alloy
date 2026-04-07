//! Seismic transaction types and utilities
use alloy_consensus::{
    transaction::{RlpEcdsaDecodableTx, RlpEcdsaEncodableTx},
    SignableTransaction, Signed, Transaction, TxLegacy, Typed2718,
};
use alloy_dyn_abi::TypedData;
use alloy_eips::{eip2930::AccessList, eip7702::SignedAuthorization};
use alloy_primitives::{
    aliases::U96, hex, keccak256, Address, Bytes, ChainId, Signature, TxKind, B256, U256,
};
use alloy_rlp::{BufMut, Decodable, Encodable};
use alloy_serde::WithOtherFields;
use core::mem;
use rand::RngCore;
use seismic_enclave::{
    ecdh_decrypt_aead, ecdh_encrypt_aead,
    secp256k1::{constants, Keypair, PublicKey, Secp256k1, SecretKey},
    Nonce,
};
use thiserror::Error;

use crate::transaction::metadata::TxSeismicMetadata;

#[cfg(feature = "serde")]
use crate::transaction::eip712::{Eip712Error, Eip712Result, TypedDataRequest};
#[cfg(feature = "serde")]
use crate::transaction::tx_serde::pubkey_with_prefix_deserialize;

/// An extension of the [`Transaction`] trait for Seismic's decryptable transactions.
pub trait InputDecryptionElements: Clone {
    /// Returns the elements necessary to decrypt the 'input' field of the transaction.
    /// May return `None` if the Seismic tx type does not support decryption.
    fn get_decryption_elements(&self) -> Result<TxSeismicElements, InputDecryptionElementsError>;

    /// Returns the 'input' field of the transaction.
    fn get_input(&self) -> Result<Bytes, InputDecryptionElementsError>;

    /// Sets the 'input' field of the transaction to the provided data.
    fn set_input(&mut self, data: Bytes) -> Result<(), InputDecryptionElementsError>;

    /// Returns tx metadata for encryption with AEAD
    fn metadata(&self, sender: Address) -> Result<TxSeismicMetadata, InputDecryptionElementsError>;

    /// Validate block-related security features (expiration and recent block hash).
    /// Non-Seismic transaction types are passed through without validation.
    fn validate_block(
        &self,
        current_block: u64,
        recent_blocks: &[B256],
    ) -> Result<(), SeismicValidationError> {
        if let Ok(elements) = self.get_decryption_elements() {
            if current_block > elements.expires_at_block {
                return Err(SeismicValidationError::TransactionExpired {
                    current_block,
                    expires_at_block: elements.expires_at_block,
                });
            }
            if !recent_blocks.contains(&elements.recent_block_hash) {
                return Err(SeismicValidationError::InvalidRecentBlockHash {
                    provided_hash: elements.recent_block_hash,
                });
            }
        }
        Ok(())
    }

    /// Creates a copy of the transaction with the input field set to the plaintext.
    /// Errors if the decryption fails, etc.
    fn plaintext_copy(
        &self,
        decryption_key: &SecretKey,
        sender: Address,
    ) -> Result<Self, InputDecryptionElementsError> {
        let mut tx = self.clone();
        if let Ok(seismic_elements) = tx.get_decryption_elements() {
            let tx_metadata = self.metadata(sender)?;
            let ciphertext = tx.get_input()?;
            let decrypted_data = seismic_elements
                .decrypt(decryption_key, &ciphertext, &tx_metadata)
                .map_err(|e| InputDecryptionElementsError::DecryptionError(e.to_string()))?;
            tx.set_input(Bytes::from(decrypted_data))?;
        }
        Ok(tx)
    }
}

/// Error type for [`InputDecryptionElements`] trait
#[derive(Debug, Clone, Error)]
pub enum InputDecryptionElementsError {
    /// The transaction type does not support decryption.
    #[error("Unsupported transaction type: {0}")]
    UnsupportedTxType(String),
    /// The decryption failed
    #[error("Decryption failed: {0}")]
    DecryptionError(String),
    /// No elements were found
    #[error("Expected Elements but no elements found")]
    NoElements,
    /// A required field is missing
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

/// Error type for seismic transaction validation
#[derive(Debug, Clone, Error)]
pub enum SeismicValidationError {
    /// Transaction has expired
    #[error(
        "Transaction expired: current block {current_block} > expiration block {expires_at_block}"
    )]
    TransactionExpired {
        /// The current block number when validation was attempted
        current_block: u64,
        /// The block number at which the transaction expires
        expires_at_block: u64,
    },
    /// Invalid recent block hash
    #[error("Invalid recent block hash: {provided_hash:?} not found in recent blocks")]
    InvalidRecentBlockHash {
        /// The block hash provided in the transaction
        provided_hash: B256,
    },
}

impl<T> InputDecryptionElements for WithOtherFields<T>
where
    T: InputDecryptionElements,
{
    fn get_decryption_elements(&self) -> Result<TxSeismicElements, InputDecryptionElementsError> {
        self.inner.get_decryption_elements()
    }

    fn get_input(&self) -> Result<alloy_primitives::Bytes, InputDecryptionElementsError> {
        self.inner.get_input()
    }

    fn set_input(
        &mut self,
        data: alloy_primitives::Bytes,
    ) -> Result<(), InputDecryptionElementsError> {
        self.inner.set_input(data)
    }

    fn metadata(&self, sender: Address) -> Result<TxSeismicMetadata, InputDecryptionElementsError> {
        self.inner.metadata(sender)
    }
}

/// Contains Seismic-specific encryption and message fields
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TxSeismicElements {
    /// The public key we will decrypt to
    #[cfg_attr(
        feature = "serde",
        serde(alias = "encryptionPubkey", deserialize_with = "pubkey_with_prefix_deserialize")
    )]
    pub encryption_pubkey: PublicKey,

    /// The nonce for the transaction
    #[cfg_attr(feature = "serde", serde(alias = "encryptionNonce"))]
    pub encryption_nonce: U96,

    /// The EIP712 version of the transaction when the user submitted it using signTypedDataV4.
    /// A value of 0 means the transaction was not signed using EIP712
    #[cfg_attr(feature = "serde", serde(alias = "messageVersion", with = "alloy_serde::quantity"))]
    pub message_version: u8,

    /// Recent block hash to prevent replay attacks across different chain states
    #[cfg_attr(feature = "serde", serde(alias = "recentBlockHash"))]
    pub recent_block_hash: B256,

    /// Block number at which this transaction expires
    #[cfg_attr(
        feature = "serde",
        serde(alias = "expiresAtBlock", alias = "expirationBlock", with = "alloy_serde::quantity")
    )]
    pub expires_at_block: u64,

    /// True if this is a signed read call, false for write transactions (default)
    #[cfg_attr(feature = "serde", serde(alias = "signedRead"))]
    pub signed_read: bool,
}

impl TxSeismicElements {
    /// Create a new TxSeismicElements with a random encryption keypair
    pub fn with_encryption_pubkey(mut self, encryption_pubkey: PublicKey) -> Self {
        self.encryption_pubkey = encryption_pubkey;
        self
    }

    /// Set the message version
    pub fn with_message_version(mut self, message_version: u8) -> Self {
        self.message_version = message_version;
        self
    }

    /// Set the encryption nonce
    pub fn with_encryption_nonce(mut self, encryption_nonce: U96) -> Self {
        self.encryption_nonce = encryption_nonce;
        self
    }

    /// Set the recent block hash
    pub fn with_recent_block_hash(mut self, recent_block_hash: B256) -> Self {
        self.recent_block_hash = recent_block_hash;
        self
    }

    /// Set the expiration block
    pub fn with_expires_at_block(mut self, expires_at_block: u64) -> Self {
        self.expires_at_block = expires_at_block;
        self
    }

    /// Set whether this is a signed read call
    pub fn with_signed_read(mut self, signed_read: bool) -> Self {
        self.signed_read = signed_read;
        self
    }

    /// Get a new encryption keypair
    pub fn get_rand_encryption_keypair() -> Keypair {
        let secp = Secp256k1::new();
        Keypair::new(&secp, &mut rand::thread_rng())
    }

    /// Get a new encryption nonce
    pub fn get_rand_encryption_nonce() -> U96 {
        let mut rng = rand::thread_rng();
        // Generate a random U96 value
        let mut bytes = [0u8; 12]; // 96 bits = 12 bytes
        rng.fill_bytes(&mut bytes);
        U96::from_be_bytes(bytes)
    }

    /// Convert U96 to Nonce
    pub fn get_enclave_nonce(self) -> Nonce {
        self.encryption_nonce.to_be_bytes().into()
    }

    /// decrypt a message using a provided secret key with AEAD and additional data
    pub fn decrypt(
        &self,
        secret_key: &SecretKey,
        ciphertext: &Bytes,
        tx_metadata: &TxSeismicMetadata,
    ) -> Result<Vec<u8>, anyhow::Error> {
        if ciphertext.is_empty() {
            return Ok(ciphertext.to_vec());
        }

        let aad = tx_metadata.encode_as_aad();
        ecdh_decrypt_aead(
            &self.encryption_pubkey,
            secret_key,
            ciphertext,
            self.get_enclave_nonce(),
            &aad,
        )
    }

    /// encrypt a message using a provided secret key
    pub fn encrypt(
        &self,
        secret_key: &SecretKey,
        plaintext: &Bytes,
        tx_metadata: &TxSeismicMetadata,
    ) -> Result<Bytes, anyhow::Error> {
        if plaintext.is_empty() {
            return Ok(plaintext.clone());
        }

        let aad = tx_metadata.encode_as_aad();
        let ciphertext = ecdh_encrypt_aead(
            &self.encryption_pubkey,
            secret_key,
            plaintext,
            self.get_enclave_nonce(),
            &aad,
        )?;
        Ok(Bytes::from(ciphertext))
    }

    /// client encrypt: network pubkey, client sk
    pub fn client_encrypt(
        &self,
        plaintext: &Bytes,
        network_pk: &PublicKey,
        client_sk: &SecretKey,
        tx_metadata: &TxSeismicMetadata,
    ) -> Result<Bytes, anyhow::Error> {
        Ok(Bytes::from(ecdh_encrypt_aead(
            network_pk,
            client_sk,
            plaintext,
            self.get_enclave_nonce(),
            &tx_metadata.encode_as_aad(),
        )?))
    }

    /// client decrypt: network pubkey, client sk
    pub fn client_decrypt(
        &self,
        ciphertext: &Bytes,
        network_pk: &PublicKey,
        client_sk: &SecretKey,
        tx_metadata: &TxSeismicMetadata,
    ) -> Result<Bytes, anyhow::Error> {
        Ok(Bytes::from(ecdh_decrypt_aead(
            network_pk,
            client_sk,
            ciphertext,
            self.get_enclave_nonce(),
            &tx_metadata.encode_as_aad(),
        )?))
    }
}

impl Default for TxSeismicElements {
    fn default() -> Self {
        Self {
            encryption_pubkey: PublicKey::from_slice(
                &hex::decode("02d211b6b0a191b9469bb3674e9c609f453d3801c3e3fd7e0bb00c6cc1e1d941df")
                    .unwrap(),
            )
            .unwrap(),
            encryption_nonce: U96::ZERO,
            message_version: 0,
            recent_block_hash: B256::ZERO,
            expires_at_block: 0,
            signed_read: false,
        }
    }
}

#[cfg(any(test, feature = "arbitrary"))]
impl<'a> arbitrary::Arbitrary<'a> for TxSeismicElements {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        // Fall back to default if we couldn't generate a valid key after several attempts
        Ok(Self {
            encryption_pubkey: Self::default().encryption_pubkey,
            message_version: u8::arbitrary(u)?,
            encryption_nonce: U96::arbitrary(u)?,
            recent_block_hash: B256::arbitrary(u)?,
            expires_at_block: u64::arbitrary(u)?,
            signed_read: bool::arbitrary(u)?,
        })
    }
}

impl Encodable for TxSeismicElements {
    fn encode(&self, out: &mut dyn BufMut) {
        self.encryption_pubkey.serialize().encode(out);
        self.encryption_nonce.encode(out);
        self.message_version.encode(out);
        self.recent_block_hash.encode(out);
        self.expires_at_block.encode(out);
        self.signed_read.encode(out);
    }

    fn length(&self) -> usize {
        self.encryption_pubkey.serialize().length() +
            self.encryption_nonce.length() +
            self.message_version.length() +
            self.recent_block_hash.length() +
            self.expires_at_block.length() +
            self.signed_read.length()
    }
}

impl Decodable for TxSeismicElements {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let pubkey_bytes: [u8; constants::PUBLIC_KEY_SIZE] = Decodable::decode(buf)?;
        Ok(Self {
            encryption_pubkey: PublicKey::from_slice(&pubkey_bytes)
                .map_err(|_| alloy_rlp::Error::Custom("invalid public key"))?,
            encryption_nonce: Decodable::decode(buf)?,
            message_version: Decodable::decode(buf)?,
            recent_block_hash: Decodable::decode(buf)?,
            expires_at_block: Decodable::decode(buf)?,
            signed_read: Decodable::decode(buf)?,
        })
    }
}

/// Basic encrypted transaction type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
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
    /// The 160-bit address of the message call's recipient or, for a contract creation
    /// transaction, ∅, used here to denote the only member of B0 ; formally Tt.
    #[cfg_attr(feature = "serde", serde(default))]
    pub to: TxKind,
    /// A scalar value equal to the number of Wei to
    /// be transerred to the message call's recipient or,
    /// in the case of contract creation, as an endowment
    /// to the newly created account; formally Tv.
    pub value: U256,
    /// Input has two uses depending if transaction is Create or Call (if `to` field is None or
    /// Some).
    ///  - pub init: An unlimited size byte array specifying the EVM-code for the account
    ///    initialisation procedure CREATE,
    ///  - data: An unlimited size byte array specifying the
    /// input data of the message call, formally Td.
    pub input: Bytes,
    /// Seismic-specific encryption and message data
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub seismic_elements: TxSeismicElements,
    /// Optional list of EIP-7702 authorization tuples for smart account delegation.
    /// Unlike TxEip7702 where an empty auth list is invalid, TxSeismic's primary purpose
    /// is encryption, so an empty list is perfectly valid (means "no delegations").
    #[cfg_attr(feature = "serde", serde(default))]
    pub authorization_list: Vec<SignedAuthorization>,
}

// We implement manually instead of using the derive macro because SignedAuthorization doesn't
// implement it.
#[cfg(any(test, feature = "arbitrary"))]
impl<'a> arbitrary::Arbitrary<'a> for TxSeismic {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            chain_id: ChainId::arbitrary(u)?,
            nonce: u64::arbitrary(u)?,
            gas_price: u128::arbitrary(u)?,
            gas_limit: u64::arbitrary(u)?,
            to: TxKind::arbitrary(u)?,
            value: U256::arbitrary(u)?,
            input: Bytes::arbitrary(u)?,
            seismic_elements: TxSeismicElements::arbitrary(u)?,
            // SignedAuthorization doesn't impl Arbitrary, so default to empty
            authorization_list: vec![],
        })
    }
}

impl TxSeismic {
    /// numeric type for the transaction
    pub const TX_TYPE: u8 = 0x4A;

    /// Get the transaction type
    pub(crate) const fn tx_type() -> u8 {
        Self::TX_TYPE
    }

    /// Returns true if the transaction is signed using EIP712
    pub fn is_eip712(&self) -> bool {
        self.seismic_elements.message_version >= 2
    }

    /// Calculates a heuristic for the in-memory size of the [`TxSeismic`] transaction.
    /// In memory stores the decrypted transaction and the encrypted transaction.
    /// Out of memory stores the encrypted transaction. This is why size and fields_len are
    /// different.
    #[inline]
    pub fn size(&self) -> usize {
        mem::size_of::<ChainId>() + // chain_id
        mem::size_of::<u64>() + // nonce
        mem::size_of::<u128>() + // gas_price
        mem::size_of::<u64>() + // gas_limit
        self.to.size() + // to
        mem::size_of::<U256>() + // value
        self.input.len() + // input
        constants::PUBLIC_KEY_SIZE + // encryption public key (33 bytes)
        mem::size_of::<U96>() + // encryption nonce (12 bytes)
        mem::size_of::<u8>() + // message_version
        mem::size_of::<B256>() + // recent_block_hash
        mem::size_of::<u64>() + // expires_at_block
        mem::size_of::<bool>() + // signed_read
        self.authorization_list.capacity() * mem::size_of::<SignedAuthorization>() // authorization_list
    }

    /// Encodes a [`TxSeismic`] into a [`TypedData`] for wallet signing via `signTypedData_v4`.
    ///
    /// Wallets (e.g. MetaMask) don't recognize the EIP-2718 `TxSeismic` type byte (`0x4a`),
    /// so they can't sign the RLP-encoded transaction directly. As a workaround, we encode
    /// the transaction as EIP-712 typed data and request a signature via `signTypedData_v4`.
    /// Seismic nodes accept both signature types (RLP and EIP-712).
    pub fn eip712_to_type_data(&self) -> TypedData {
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
                  // EIP-712 doesn't support optional types, so we can't have Optional(address) to encode a CREATE tx as not having a `to` field.
                  // Instead we force CREATE to serialize as (to=0x0, isCreate=true).
                  // See eip712_decode for the consistency validation.
                  { "name": "to", "type": "address" },
                  { "name": "isCreate", "type": "bool" },
                  { "name": "value", "type": "uint256" },
                  // compressed secp256k1 public key (33 bytes)
                  { "name": "input", "type": "bytes" },
                  { "name": "encryptionPubkey", "type": "bytes" },
                  { "name": "encryptionNonce", "type": "uint96" },
                  { "name": "messageVersion", "type": "uint8" },
                  { "name": "recentBlockHash", "type": "bytes32" },
                  { "name": "expiresAtBlock", "type": "uint64" },
                  { "name": "signedRead", "type": "bool" },
                ],
            },
            "primaryType": "TxSeismic",
            "domain": {
                "name": "Seismic Transaction",
                "version": self.seismic_elements.message_version.to_string(),
                "chainId": self.chain_id,
                // no verifying contract since this happens in RPC
                "verifyingContract": "0x0000000000000000000000000000000000000000",
            },
            "message": {
                "chainId": self.chain_id.to_string(),
                "nonce": self.nonce.to_string(),
                "gasPrice": self.gas_price.to_string(),
                "gasLimit": self.gas_limit,
                "to": match self.to {
                    TxKind::Create => Address::ZERO.to_string(),
                    TxKind::Call(to) => to.to_string(),
                },
                "isCreate": self.to.is_create(),
                "value": self.value.to_string(),
                "input": self.input.to_string(),
                "encryptionPubkey": self.seismic_elements.encryption_pubkey.to_string(),
                "encryptionNonce": self.seismic_elements.encryption_nonce.to_string(),
                "messageVersion": self.seismic_elements.message_version.to_string(),
                "recentBlockHash": self.seismic_elements.recent_block_hash.to_string(),
                "expiresAtBlock": self.seismic_elements.expires_at_block.to_string(),
                "signedRead": self.seismic_elements.signed_read,
            }
        });
        serde_json::from_value(typed_data_json)
            .map_err(|e| format!("Failed to convert seismic transaction to typed data: {e}"))
            .unwrap()
    }

    #[cfg(feature = "serde")]
    /// Decodes a [`TypedData`] into a [`TxSeismic`].
    pub fn eip712_decode(typed_data: &TypedData) -> Eip712Result<Self> {
        // Extract the `message` field from TypedData (JSON format)
        let message = serde_json::to_value(&typed_data.message)
            .map_err(|_| Eip712Error::DecodeError("Failed to serialize message".to_string()))?;

        // Extract the explicit isCreate flag
        let is_create = message.get("isCreate").and_then(|v| v.as_bool()).ok_or_else(|| {
            Eip712Error::DecodeError("Missing or invalid isCreate field".to_string())
        })?;

        // Deserialize JSON `message` into `TxSeismic`
        let mut tx: TxSeismic = serde_json::from_value(message)
            .map_err(|_| Eip712Error::DecodeError("Failed to deserialize message".to_string()))?;

        // The EIP-712 schema declares `to` as type `address`, so it must always be
        // present and valid. TxKind::Create is encoded as (to=0x0, isCreate=true) —
        // see eip712_to_type_data. Reject null/missing `to` (which serde defaults
        // to TxKind::Create).
        let TxKind::Call(to_addr) = tx.to else {
            return Err(Eip712Error::DecodeError(
                "to must be a valid address - create txs should use the isCreate field".to_string(),
            ));
        };

        if is_create {
            if to_addr != Address::ZERO {
                return Err(Eip712Error::DecodeError(
                    "isCreate is true but to is not the zero address".to_string(),
                ));
            }
            tx.to = TxKind::Create;
        }

        Ok(tx)
    }

    fn eip712_signature_hash(&self) -> B256 {
        let typed_data = self.eip712_to_type_data();

        typed_data.eip712_signing_hash().expect("Failed to hash seismic transaction in eip712")
    }

    /// Convert a [`TxSeismic`] to a [`TxLegacy`] transaction (dropping the seismic elements)
    pub fn to_legacy_tx(&self) -> TxLegacy {
        TxLegacy {
            chain_id: Some(self.chain_id),
            nonce: self.nonce,
            gas_price: self.gas_price,
            gas_limit: self.gas_limit,
            to: self.to,
            value: self.value,
            input: self.input.clone(),
        }
    }

    /// Extract legacy transaction fields
    pub fn legacy_fields(&self) -> crate::TxLegacyFields {
        crate::TxLegacyFields {
            chain_id: self.chain_id,
            nonce: self.nonce,
            to: self.to,
            value: self.value,
        }
    }

    /// Create metadata for AEAD encryption
    pub fn tx_metadata(&self, sender: Address) -> TxSeismicMetadata {
        TxSeismicMetadata {
            sender,
            legacy_fields: self.legacy_fields(),
            seismic_elements: self.seismic_elements,
        }
    }

    /// Encrypt input data with AEAD using transaction metadata
    /// This is the main method for encrypting transaction calldata
    pub fn encrypt_input_aead(
        &self,
        secret_key: &SecretKey,
        plaintext: &Bytes,
        sender: Address,
    ) -> Result<Bytes, anyhow::Error> {
        let metadata = self.tx_metadata(sender);
        self.seismic_elements.encrypt(secret_key, plaintext, &metadata)
    }

    /// Decrypt input data with AEAD using transaction metadata
    /// This is the main method for decrypting transaction calldata
    pub fn decrypt_input_aead(
        &self,
        secret_key: &SecretKey,
        ciphertext: &Bytes,
        sender: Address,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let metadata = self.tx_metadata(sender);
        self.seismic_elements.decrypt(secret_key, ciphertext, &metadata)
    }

    /// Validate that the recent block hash is in the provided list of recent blocks
    /// Returns true if the block hash is found in the recent blocks list
    pub fn validate_recent_block_hash(&self, recent_blocks: &[B256]) -> bool {
        recent_blocks.contains(&self.seismic_elements.recent_block_hash)
    }

    /// Validate that the transaction has not expired
    /// Returns true if current_block <= expires_at_block
    pub fn validate_expiration(&self, current_block: u64) -> bool {
        current_block <= self.seismic_elements.expires_at_block
    }

    /// Validate block-related security features (expiration and recent block hash).
    /// Delegates to [`InputDecryptionElements::validate_block`].
    pub fn validate_block(
        &self,
        current_block: u64,
        recent_blocks: &[B256],
    ) -> Result<(), SeismicValidationError> {
        InputDecryptionElements::validate_block(self, current_block, recent_blocks)
    }
}

#[cfg(feature = "serde")]
impl From<Signed<TxSeismic>> for TypedDataRequest {
    fn from(tx: Signed<TxSeismic>) -> Self {
        TypedDataRequest { data: tx.tx().eip712_to_type_data(), signature: *tx.signature() }
    }
}

impl RlpEcdsaEncodableTx for TxSeismic {
    fn rlp_encoded_fields_length(&self) -> usize {
        self.chain_id.length() +
            self.nonce.length() +
            self.gas_price.length() +
            self.gas_limit.length() +
            self.to.length() +
            self.value.length() +
            self.seismic_elements.length() +
            self.input.length() +
            self.authorization_list.length()
    }

    fn rlp_encode_fields(&self, out: &mut dyn alloy_rlp::BufMut) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.gas_price.encode(out);
        self.gas_limit.encode(out);
        self.to.encode(out);
        self.value.encode(out);
        self.seismic_elements.encode(out);
        self.input.encode(out);
        self.authorization_list.encode(out);
    }

    fn tx_hash(&self, signature: &Signature) -> alloy_primitives::TxHash {
        /*
        // While this will work, it's unclear whether we want this
        if self.is_eip712() {
            let mut bytes = vec![];
            self.encode_for_signing(&mut bytes);
            self.rlp_encode_signed(&signature, &mut bytes);
            return keccak256(bytes.as_slice());
        }
        */
        self.tx_hash_with_type(signature, self.ty())
    }
}

impl RlpEcdsaDecodableTx for TxSeismic {
    const DEFAULT_TX_TYPE: u8 = { Self::tx_type() as u8 };

    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Ok(Self {
            chain_id: Decodable::decode(buf)?,
            nonce: Decodable::decode(buf)?,
            gas_price: Decodable::decode(buf)?,
            gas_limit: Decodable::decode(buf)?,
            to: Decodable::decode(buf)?,
            value: Decodable::decode(buf)?,
            seismic_elements: Decodable::decode(buf)?,
            input: Decodable::decode(buf)?,
            authorization_list: Decodable::decode(buf)?,
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
        Some(&self.authorization_list)
    }
}

impl InputDecryptionElements for TxSeismic {
    fn get_decryption_elements(&self) -> Result<TxSeismicElements, InputDecryptionElementsError> {
        Ok(self.seismic_elements)
    }

    fn get_input(&self) -> Result<Bytes, InputDecryptionElementsError> {
        Ok(self.input.clone())
    }

    fn set_input(&mut self, data: Bytes) -> Result<(), InputDecryptionElementsError> {
        self.input = data;
        Ok(())
    }

    fn metadata(&self, sender: Address) -> Result<TxSeismicMetadata, InputDecryptionElementsError> {
        Ok(self.tx_metadata(sender))
    }
}

impl Typed2718 for TxSeismic {
    fn ty(&self) -> u8 {
        Self::tx_type() as u8
    }
}

impl SignableTransaction<Signature> for TxSeismic {
    fn set_chain_id(&mut self, chain_id: ChainId) {
        self.chain_id = chain_id;
    }

    fn encode_for_signing(&self, out: &mut dyn alloy_rlp::BufMut) {
        if self.is_eip712() {
            let data = self
                .eip712_to_type_data()
                .encode_data()
                .expect("Failed to encode seismic transaction for signing");
            out.put_slice(data.as_slice());
        } else {
            out.put_u8(Self::tx_type() as u8);
            self.encode(out)
        }
    }

    fn payload_len_for_signature(&self) -> usize {
        if self.is_eip712() {
            let data = self
                .eip712_to_type_data()
                .encode_data()
                .expect("Failed to encode seismic transaction for signature length");
            data.len()
        } else {
            self.length() + 1
        }
    }

    fn into_signed(self, signature: Signature) -> Signed<Self> {
        let tx_hash = self.tx_hash(&signature);
        Signed::new_unchecked(self, signature, tx_hash)
    }

    fn signature_hash(&self) -> B256 {
        if self.is_eip712() {
            self.eip712_signature_hash()
        } else {
            keccak256(self.encoded_for_signing())
        }
    }
}

// #[cfg(feature = "k256")]
// impl Signed<TxSeismic> {
//     /// If this was a signed call, recover the caller's address
//     /// Main difference is we have to change the EIP domain name
//     /// to be "Signed Call" instead of "Seismic Transaction"
//     pub fn recover_caller(
//         &self,
//     ) -> Result<alloy_primitives::Address, alloy_primitives::SignatureError> {
//         let tx = self.tx();
//         if !tx.is_eip712() {
//             return self.recover_signer();
//         }
//         let tx_hash: alloy_primitives::FixedBytes<32> = tx.eip712_signature_hash();
//         self.signature().recover_address_from_prehash(&tx_hash)
//     }
// } // TODO: this probably needs to be in txenvelope

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
    use std::borrow::Cow;

    use alloy_primitives::{Bytes, ChainId, TxKind, U256};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_with::{DeserializeAs, SerializeAs};

    use alloy_eips::eip7702::SignedAuthorization;

    use super::TxSeismicElements;

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
        seismic_elements: TxSeismicElements,
        input: Cow<'a, Bytes>,
        #[serde(default)]
        authorization_list: Cow<'a, Vec<SignedAuthorization>>,
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
                seismic_elements: value.seismic_elements,
                input: Cow::Borrowed(&value.input),
                authorization_list: Cow::Borrowed(&value.authorization_list),
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
                seismic_elements: value.seismic_elements,
                input: value.input.into_owned(),
                authorization_list: value.authorization_list.into_owned(),
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
        fn test_tx_seismic_bincode_roundtrip() {
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
    use std::str::FromStr;

    use alloy_primitives::{
        b256,
        hex::{self, FromHex},
        Address, FixedBytes, Signature,
    };
    use seismic_enclave::{get_unsecure_sample_secp256k1_pk, get_unsecure_sample_secp256k1_sk};

    use super::*;

    #[cfg(feature = "serde")]
    use k256::ecdsa::SigningKey;

    #[test]
    fn test_encode_decode_public_key() {
        let pubkey = TxSeismicElements::get_rand_encryption_keypair().public_key();
        let encoded = pubkey.serialize();
        let decoded = PublicKey::from_slice(&encoded).unwrap();
        assert_eq!(pubkey, decoded);
    }

    #[test]
    fn test_encode_decode_seismic() {
        let hash: B256 =
            b256!("0xd18462df79a0e613bf39886c23ef1e0fc0c7d518b488fa5e76bf7462632cc1f9");

        let tx = TxSeismic {
            chain_id: 4u64,
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: Address::from_slice(&hex!("d3e8763675e4c425df46cc3b5c0f6cbdac396046")).into(),
            value: U256::from(1000000000000000u64),
            seismic_elements: TxSeismicElements::default(),
            input:  hex!("a22cb4650000000000000000000000005eee75727d804a2b13038928d36f8b188945a57a0000000000000000000000000000000000000000000000000000000000000000").into(),
            authorization_list: vec![],
        };

        let sig = Signature::from_scalars_and_parity(
            b256!("840cfc572845f5786e702984c2a582528cad4b49b2a10b9db1be7fca90058565"),
            b256!("25e7109ceb98168d95b09b18bbf6b685130e0562f233877d492b94eee0c5b6d1"),
            false,
        );

        let mut buf = vec![];
        tx.rlp_encode_signed(&sig, &mut buf);
        let decoded = TxSeismic::rlp_decode_signed(&mut &buf[..]).unwrap();
        assert_eq!(decoded, tx.clone().into_signed(sig));
        assert_eq!(decoded.tx().clone(), tx.clone());
        assert_eq!(*decoded.hash(), hash);

        #[cfg(feature = "k256")]
        {
            let signer = decoded.recover_signer().unwrap();
            assert_eq!(
                signer,
                Address::from_slice(&hex!("166cd796821ac9f47605dcf63c313f5a0df355e7"))
            );
        }
    }

    #[cfg(feature = "serde")]
    fn get_signing_private_key() -> SigningKey {
        let private_key_bytes =
            hex!("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80");
        let signing_key =
            SigningKey::from_bytes(&private_key_bytes.into()).expect("Invalid private key");
        signing_key
    }

    #[cfg(feature = "serde")]
    fn get_signing_address() -> Address {
        Address::from_public_key(&get_signing_private_key().verifying_key())
    }

    #[cfg(feature = "serde")]
    /// Sign a seismic transaction
    fn sign_hash(msg: &[u8]) -> Signature {
        let _signature = get_signing_private_key()
            .clone()
            .sign_prehash_recoverable(msg)
            .expect("Failed to sign");

        let recoverid = _signature.1;
        let _signature = _signature.0;

        let signature = Signature::new(
            U256::from_be_slice(_signature.r().to_bytes().as_ref()),
            U256::from_be_slice(_signature.s().to_bytes().as_ref()),
            recoverid.is_y_odd(),
        );

        signature
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_eip712_encode_decode() {
        let tx = TxSeismic {
            chain_id: 4u64,
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: TxKind::Create,
            value: U256::from(1000000000000000u64),
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::get_rand_encryption_keypair().public_key(),
                encryption_nonce: U96::from(1),
                message_version: 2,
                recent_block_hash: B256::from_slice(&hex!("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef")),
                expires_at_block: 1000000,
                signed_read: false,
            },
            authorization_list: vec![],
            input:  hex!("a22cb4650000000000000000000000005eee75727d804a2b13038928d36f8b188945a57a0000000000000000000000000000000000000000000000000000000000000000").into(),
        };
        let typed_data = tx.eip712_to_type_data();
        let decoded = TxSeismic::eip712_decode(&typed_data).unwrap();
        assert_eq!(decoded, tx);

        // signing
        let signature_hash = tx.signature_hash();
        let sig = sign_hash(&signature_hash.as_slice());

        assert_eq!(
            Address::from_public_key(&sig.recover_from_prehash(&signature_hash).unwrap()),
            get_signing_address()
        );

        let signed = tx.clone().into_signed(sig);

        assert_eq!(signed.tx(), &tx);
        assert_eq!(signed.signature(), &sig);
        assert_ne!(*signed.hash(), signature_hash);

        let typed_data_request: TypedDataRequest = signed.into();
        assert_eq!(typed_data_request.data, tx.eip712_to_type_data());
        assert_eq!(typed_data_request.signature, sig);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_eip712_encode_decode_max_value() {
        // when the value for gas_price is too large, json! macro cannot handle it
        let tx = TxSeismic {
            chain_id: u64::max_value(),
            nonce: u64::max_value(),
            gas_price: u128::max_value(),
            gas_limit: u64::max_value(),
            to: TxKind::Call(Address::from_slice(&hex!(
                "87d40d7c65ef908b24cf2a0ddf0b620ebca686b5"
            ))),
            value: U256::MAX,
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::get_rand_encryption_keypair().public_key(),
                message_version: u8::max_value(),
                encryption_nonce: U96::MAX,
                recent_block_hash: B256::repeat_byte(0xff),
                expires_at_block: u64::max_value(),
                signed_read: true,
            },
            input: Bytes::default(),
            authorization_list: vec![],
        };
        let typed_data = tx.eip712_to_type_data();
        let decoded = TxSeismic::eip712_decode(&typed_data).unwrap();
        assert_eq!(decoded, tx);

        let _signature_hash = decoded.eip712_signature_hash();
    }

    // Verify that Call(Address::ZERO) survives EIP-712 encode/decode round-trip.
    // We at some point had a bug where this would get serialized to a CREATE tx.
    #[cfg(feature = "serde")]
    #[test]
    fn test_eip712_call_zero_address_round_trip() {
        let tx = TxSeismic {
            chain_id: 1u64,
            nonce: 42,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(Address::ZERO),
            value: U256::from(1u64),
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::default().encryption_pubkey,
                encryption_nonce: U96::from(1),
                message_version: 2,
                recent_block_hash: B256::ZERO,
                expires_at_block: 100,
                signed_read: false,
            },
            input: Bytes::default(),
            authorization_list: vec![],
        };

        let typed_data = tx.eip712_to_type_data();
        let decoded = TxSeismic::eip712_decode(&typed_data).unwrap();

        assert_eq!(tx.to, TxKind::Call(Address::ZERO));
        assert_eq!(decoded.to, TxKind::Call(Address::ZERO));
        assert_eq!(decoded, tx);
    }

    // Verify that isCreate=true with a non-zero `to` address is rejected.
    #[cfg(feature = "serde")]
    #[test]
    fn test_eip712_decode_rejects_is_create_with_nonzero_to() {
        // Start with a valid Create tx, encode it, then tamper with the `to` field
        let tx = TxSeismic {
            chain_id: 1u64,
            nonce: 1,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Create,
            value: U256::ZERO,
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::default().encryption_pubkey,
                encryption_nonce: U96::from(1),
                message_version: 2,
                recent_block_hash: B256::ZERO,
                expires_at_block: 100,
                signed_read: false,
            },
            input: Bytes::default(),
            authorization_list: vec![],
        };

        let mut typed_data = tx.eip712_to_type_data();

        // Tamper: set `to` to a non-zero address while isCreate remains true
        typed_data.message.as_object_mut().unwrap().insert(
            "to".to_string(),
            serde_json::Value::String("0x0000000000000000000000000000000000000001".to_string()),
        );

        let result = TxSeismic::eip712_decode(&typed_data);
        assert!(result.is_err(), "should reject isCreate=true with non-zero to address");
    }

    // Verify that isCreate=false with a null `to` field is rejected.
    // TxKind::Create is the serde default, so null/missing `to` silently
    // deserializes as Create — the isCreate flag must catch this.
    #[cfg(feature = "serde")]
    #[test]
    fn test_eip712_decode_rejects_null_to_with_is_create_false() {
        // Start with a valid Call tx, encode it, then tamper
        let tx = TxSeismic {
            chain_id: 1u64,
            nonce: 1,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: TxKind::Call(Address::with_last_byte(1)),
            value: U256::ZERO,
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::default().encryption_pubkey,
                encryption_nonce: U96::from(1),
                message_version: 2,
                recent_block_hash: B256::ZERO,
                expires_at_block: 100,
                signed_read: false,
            },
            input: Bytes::default(),
            authorization_list: vec![],
        };

        let mut typed_data = tx.eip712_to_type_data();

        // Tamper: set `to` to null while isCreate remains false
        typed_data
            .message
            .as_object_mut()
            .unwrap()
            .insert("to".to_string(), serde_json::Value::Null);
        let result = TxSeismic::eip712_decode(&typed_data);
        assert!(result.is_err(), "should reject isCreate=false with null to");

        // Tamper: remove `to` entirely while isCreate remains false
        typed_data.message.as_object_mut().unwrap().remove("to");
        let result = TxSeismic::eip712_decode(&typed_data);
        assert!(result.is_err(), "should reject isCreate=false with missing to");
    }

    #[test]
    fn test_eip712_hash() {
        let tx = TxSeismic {
            chain_id: 5124,
            nonce: 48,
            gas_price: 360000,
            gas_limit: 169477,
            to: alloy_primitives::TxKind::Call(Address::from_str("0x3aB946eEC2553114040dE82D2e18798a51cf1e14").unwrap()),
            value: U256::from_str("1000000000000000").unwrap(),
            input: Bytes::from_str("0x4e69e56c3bb999b8c98772ebb32aebcbd43b33e9e65a46333dfe6636f37f3009e93bad334235aec73bd54d11410e64eb2cab4da8").unwrap(),
            seismic_elements: TxSeismicElements {
                encryption_pubkey: PublicKey::from_str("028e76821eb4d77fd30223ca971c49738eb5b5b71eabe93f96b348fdce788ae5a0").unwrap(),
                encryption_nonce: U96::from_str("0x7da3a99bf0f90d56551d99ea").unwrap(),
                message_version: 2,
                recent_block_hash: B256::from_slice(&hex!("abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890")),
                expires_at_block: 1000000,
                signed_read: false,
            },
            authorization_list: vec![],
        };
        let signature = {
            let r_bytes =
                hex::decode("e93185920818650416b4b0cc953c48f59fd9a29af4b7e1c4b1ac4824392f9220")
                    .unwrap();
            let s_bytes =
                hex::decode("79b76b064a83d423997b7234c575588f60da5d3e1e0561eff9804eb04c23789a")
                    .unwrap();
            let mut r_padded = [0u8; 32];
            let mut s_padded = [0u8; 32];
            let r_start = 32 - r_bytes.len();
            let s_start = 32 - s_bytes.len();

            r_padded[r_start..].copy_from_slice(&r_bytes);
            s_padded[s_start..].copy_from_slice(&s_bytes);

            let r = U256::from_be_bytes(r_padded);
            let s = U256::from_be_bytes(s_padded);

            Signature::new(r, s, false)
        };
        let signed = tx.clone().into_signed(signature);
        let signed_hash = signed.hash();

        let expected_tx_hash = FixedBytes::<32>::from_hex(
            "0x8c95f5133ab8d55531621f1d46f0ca092084be09db7a932e738c56003d3735eb",
        )
        .unwrap();

        assert_eq!(signed_hash, &expected_tx_hash);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_pubkey_with_prefix_hex() {
        let without_prefix_pubkey =
            "\"03c5e6d6b9916ee954b9724be6e31623c80c1fbe598aac48dcc075a7023077d44b\"";
        let key: PublicKey = serde_json::from_str(without_prefix_pubkey).unwrap();

        let raw = "{\"encryptionPubkey\":\"0x03c5e6d6b9916ee954b9724be6e31623c80c1fbe598aac48dcc075a7023077d44b\",\"encryptionNonce\":\"0x5ca801ecf9742c75e30cb9ed\",\"messageVersion\":\"0x0\",\"recentBlockHash\":\"0x0000000000000000000000000000000000000000000000000000000000000000\",\"expiresAtBlock\":\"0x0\",\"signedRead\":false}";

        let with_prefix_pubkey: TxSeismicElements = serde_json::from_str(raw).unwrap();
        assert_eq!(with_prefix_pubkey.encryption_pubkey, key);

        // Test backward compatibility with old field name
        let raw_old = "{\"encryptionPubkey\":\"0x03c5e6d6b9916ee954b9724be6e31623c80c1fbe598aac48dcc075a7023077d44b\",\"encryptionNonce\":\"0x5ca801ecf9742c75e30cb9ed\",\"messageVersion\":\"0x0\",\"recentBlockHash\":\"0x0000000000000000000000000000000000000000000000000000000000000000\",\"expirationBlock\":\"0x0\",\"signedRead\":false}";
        let with_old_field: TxSeismicElements = serde_json::from_str(raw_old).unwrap();
        assert_eq!(with_old_field.encryption_pubkey, key);
        assert_eq!(with_old_field.expires_at_block, 0);
    }

    #[test]
    fn test_encrypt_empty_bytes() {
        let sender = Address::ZERO;
        let seismic_elements = TxSeismicElements::default();
        let empty_bytes = Bytes::new();

        let tx_io_sk = get_unsecure_sample_secp256k1_sk();
        let tx_metadata = TxSeismicMetadata::example(seismic_elements.clone(), sender);

        let result = seismic_elements.encrypt(&tx_io_sk, &empty_bytes, &tx_metadata);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Bytes::new());
    }

    #[test]
    fn test_seismic_calldata_encoding() {
        // similar test to seismic-viem-test's trace.ts
        let plaintext = Bytes::from_str("0xdeadbeef").unwrap();
        let seismic_elements = TxSeismicElements {
            encryption_pubkey: get_unsecure_sample_secp256k1_pk(),
            encryption_nonce: U96::MAX,
            message_version: 0,
            recent_block_hash: FixedBytes::<32>::from_str(
                "0x3a7c05da853bd4c4683023e3ba72a81e1015a60aab8b12218f033c0d6544d10e",
            )
            .unwrap(),
            expires_at_block: 100,
            signed_read: false,
        };
        let secret_key = get_unsecure_sample_secp256k1_sk();
        let orig_decoded_tx = TxSeismic {
            chain_id: 31337u64,
            nonce: 0,
            gas_price: 10_000_000_000,
            gas_limit: 30_000_000,
            to: Address::ZERO.into(),
            value: U256::from(1u64),
            seismic_elements,
            input: plaintext.clone(),
            authorization_list: vec![],
        };
        let sender = Address::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266").unwrap();
        let tx_metadata = orig_decoded_tx.metadata(sender).unwrap();
        let encoded_metadata = Bytes::from(tx_metadata.encode_as_aad());
        let expected_emd = Bytes::from_hex("0xf88294f39fd6e51aad88f6f4ce6ab8827279cfffb92266827a698094000000000000000000000000000000000000000001a1028e76821eb4d77fd30223ca971c49738eb5b5b71eabe93f96b348fdce788ae5a08cffffffffffffffffffffffff80a03a7c05da853bd4c4683023e3ba72a81e1015a60aab8b12218f033c0d6544d10e6480").unwrap();
        assert_eq!(encoded_metadata, expected_emd);
        let encrypted_calldata =
            seismic_elements.encrypt(&secret_key, &plaintext, &tx_metadata).unwrap();
        let expected_ecd = Bytes::from_hex("0x12fbf3f819e7ae972bfedfc6a5a249983ae527e0").unwrap();
        assert_eq!(encrypted_calldata, expected_ecd);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_payload_len_matches_encoded_for_signing_eip712() {
        let tx = TxSeismic {
            chain_id: 4u64,
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: TxKind::Create,
            value: U256::from(1000000000000000u64),
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::get_rand_encryption_keypair().public_key(),
                encryption_nonce: U96::from(1),
                message_version: 2, // EIP-712 mode
                recent_block_hash: B256::ZERO,
                expires_at_block: 100,
                signed_read: false,
            },
            input: Bytes::from_str("0xdeadbeef").unwrap(),
            authorization_list: vec![],
        };

        let mut buf = vec![];
        tx.encode_for_signing(&mut buf);
        assert_eq!(tx.payload_len_for_signature(), buf.len());
    }

    #[test]
    fn test_payload_len_matches_encoded_for_signing_legacy() {
        let tx = TxSeismic {
            chain_id: 4u64,
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: TxKind::Call(Address::ZERO),
            value: U256::from(1u64),
            seismic_elements: TxSeismicElements {
                encryption_pubkey: TxSeismicElements::get_rand_encryption_keypair().public_key(),
                encryption_nonce: U96::from(1),
                message_version: 0, // legacy mode
                recent_block_hash: B256::ZERO,
                expires_at_block: 100,
                signed_read: false,
            },
            input: Bytes::default(),
            authorization_list: vec![],
        };

        let mut buf = vec![];
        tx.encode_for_signing(&mut buf);
        assert_eq!(tx.payload_len_for_signature(), buf.len());
    }
}
