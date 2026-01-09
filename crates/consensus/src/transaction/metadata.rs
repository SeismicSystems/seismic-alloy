//! Transaction metadata for AEAD encryption

use alloy_primitives::{Address, Bytes, ChainId, TxKind, U256};
use alloy_rlp::Encodable;
use secp256k1::SecretKey;

use super::seismic::TxSeismicElements;

/// Legacy transaction fields used in metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxLegacyFields {
    /// Chain ID
    pub chain_id: ChainId,
    /// Transaction nonce
    pub nonce: u64,
    /// Transaction recipient or create flag
    pub to: TxKind,
    /// Transaction value
    pub value: U256,
}

impl TxLegacyFields {
    /// Encode legacy fields to a buffer
    pub fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.to.encode(out);
        self.value.encode(out);
    }
}

/// Transaction metadata used for AEAD additional authenticated data
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxSeismicMetadata {
    /// EOA who signed the transaction
    pub sender: Address,
    /// Legacy transaction fields
    pub legacy_fields: TxLegacyFields,
    /// All seismic elements (includes security fields and encryption params)
    pub seismic_elements: TxSeismicElements,
}

impl TxSeismicMetadata {
    /// Encode the metadata as additional authenticated data for AEAD
    /// Encodes all fields as a single RLP list for easy decoding
    pub fn encode_as_aad(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        self.sender.encode(&mut payload);
        self.legacy_fields.encode(&mut payload);
        self.seismic_elements.encode(&mut payload);

        // Wrap the concatenated fields in an RLP list header
        let header = alloy_rlp::Header { list: true, payload_length: payload.len() };
        let mut out = Vec::new();
        header.encode(&mut out);
        out.extend_from_slice(&payload);
        out
    }

    /// encrypt plaintext calldata using AEAD
    pub fn encrypt(
        &self,
        secret_key: &SecretKey,
        plaintext: &Bytes,
    ) -> Result<Bytes, anyhow::Error> {
        self.seismic_elements.encrypt(secret_key, plaintext, self)
    }

    /// decrypt plaintext calldata using AEAD
    pub fn decrypt(
        &self,
        secret_key: &SecretKey,
        ciphertext: &Bytes,
    ) -> Result<Vec<u8>, anyhow::Error> {
        self.seismic_elements.decrypt(secret_key, ciphertext, self)
    }

    #[cfg(test)]
    /// Metadata for testing
    pub fn example(seismic_elements: TxSeismicElements, sender: Address) -> TxSeismicMetadata {
        TxSeismicMetadata {
            legacy_fields: TxLegacyFields {
                chain_id: 5124,
                nonce: 0,
                to: TxKind::Call(Address::ZERO),
                value: U256::ZERO,
            },
            seismic_elements,
            sender,
        }
    }
}
