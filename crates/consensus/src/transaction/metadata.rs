//! Transaction metadata for AEAD encryption

use alloy_primitives::{Address, Bytes, ChainId, TxKind, U256};
use alloy_rlp::Encodable;
use seismic_crypto::secp256k1::{PublicKey, SecretKey};

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

    /// Encode the response AAD: the request AAD with the response format version appended.
    pub fn encode_response_aad(&self, version: u8) -> Vec<u8> {
        let mut aad = self.encode_as_aad();
        aad.push(version);
        aad
    }

    /// TEE-side encryption of a signed-read result using the response traffic key
    pub fn encrypt_response(
        &self,
        secret_key: &SecretKey,
        plaintext: &Bytes,
    ) -> Result<Bytes, anyhow::Error> {
        self.seismic_elements.encrypt_response(secret_key, plaintext, self)
    }

    /// TEE-side decryption of request calldata using the request traffic key
    pub fn decrypt_request(
        &self,
        secret_key: &SecretKey,
        ciphertext: &Bytes,
    ) -> Result<Vec<u8>, anyhow::Error> {
        self.seismic_elements.decrypt_request(secret_key, ciphertext, self)
    }

    /// client-side encrypt: takes TEE public key and provider secret key
    /// This is the method that should be used when encrypting transaction calldata
    /// from the client side (before sending to the network)
    pub fn client_encrypt(
        &self,
        plaintext: &Bytes,
        network_pk: &PublicKey,
        client_sk: &SecretKey,
    ) -> Result<Bytes, anyhow::Error> {
        self.seismic_elements.client_encrypt(plaintext, network_pk, client_sk, self)
    }

    /// client-side decrypt: takes TEE public key and provider secret key
    /// This is the method that should be used when decrypting transaction calldata
    /// from the client side (after receiving from thenetwork)
    pub fn client_decrypt(
        &self,
        ciphertext: &Bytes,
        network_pk: &PublicKey,
        client_sk: &SecretKey,
    ) -> Result<Bytes, anyhow::Error> {
        self.seismic_elements.client_decrypt(ciphertext, network_pk, client_sk, self)
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
