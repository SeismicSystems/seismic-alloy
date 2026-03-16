//! Helpers for calling Seismic's custom precompiles from client code.
//!
//! Seismic extends the EVM with six precompiles at addresses `0x64`–`0x69`
//! for on-chain cryptography: random number generation, ECDH key agreement,
//! AES-GCM encryption/decryption, HKDF key derivation, and ECDSA signing.
//!
//! This module provides:
//! - [`addresses`] — precompile address constants
//! - Encoding/decoding helpers for each precompile's raw byte format
//! - [`call`] — convenience wrappers that encode, `eth_call`, and decode in one step
//!
//! # Example
//!
//! ```rust,ignore
//! use seismic_alloy_provider::precompiles;
//!
//! // Generate 32 bytes of randomness via the RNG precompile
//! let random_bytes = precompiles::call::rng(&provider, 32, b"my_domain").await?;
//!
//! // Derive a shared AES key via ECDH
//! let aes_key = precompiles::call::ecdh(&provider, &secret_key, &public_key).await?;
//! ```

use alloy_primitives::{address, Address, Bytes, FixedBytes};

/// Precompile contract addresses.
pub mod addresses {
    use super::*;

    /// RNG — on-chain random number generation (gas: 3500 + 5 per 32-byte word).
    pub const RNG: Address = address!("0x0000000000000000000000000000000000000064");

    /// ECDH — derive a shared AES-256 key from a secret key and public key (gas: 3120).
    pub const ECDH: Address = address!("0x0000000000000000000000000000000000000065");

    /// AES-256-GCM encryption (gas: 1000 + 30 per 16-byte block).
    pub const AES_ENCRYPT: Address = address!("0x0000000000000000000000000000000000000066");

    /// AES-256-GCM decryption (gas: 1000 + 30 per 16-byte block).
    pub const AES_DECRYPT: Address = address!("0x0000000000000000000000000000000000000067");

    /// HKDF — derive an AES-256 key from arbitrary input (variable gas).
    pub const HKDF: Address = address!("0x0000000000000000000000000000000000000068");

    /// SECP256K1 — ECDSA recoverable signing (gas: 3000).
    pub const SECP256K1_SIGN: Address = address!("0x0000000000000000000000000000000000000069");
}

// ---------------------------------------------------------------------------
// Input encoding
// ---------------------------------------------------------------------------

/// Encode input for the RNG precompile (0x64).
///
/// - `output_len`: number of random bytes to generate
/// - `personalization`: domain separation bytes (must be non-empty)
///
/// Format: `[u32 BE output_len | personalization...]`
pub fn encode_rng(output_len: u32, personalization: &[u8]) -> Bytes {
    let mut buf = Vec::with_capacity(4 + personalization.len());
    buf.extend_from_slice(&output_len.to_be_bytes());
    buf.extend_from_slice(personalization);
    buf.into()
}

/// Encode input for the ECDH precompile (0x65).
///
/// - `secret_key`: 32-byte secp256k1 secret key
/// - `compressed_pubkey`: 33-byte compressed secp256k1 public key
///
/// Format: `[secret_key (32) | compressed_pubkey (33)]` → 65 bytes total
pub fn encode_ecdh(secret_key: &FixedBytes<32>, compressed_pubkey: &[u8; 33]) -> Bytes {
    let mut buf = Vec::with_capacity(65);
    buf.extend_from_slice(secret_key.as_slice());
    buf.extend_from_slice(compressed_pubkey);
    buf.into()
}

/// Encode input for AES-256-GCM encryption (0x66).
///
/// - `key`: 32-byte AES-256 key
/// - `nonce`: 12-byte nonce (96-bit, big-endian)
/// - `plaintext`: data to encrypt
///
/// Format: `[key (32) | nonce (12) | plaintext...]`
pub fn encode_aes_encrypt(
    key: &FixedBytes<32>,
    nonce: &FixedBytes<12>,
    plaintext: &[u8],
) -> Bytes {
    let mut buf = Vec::with_capacity(44 + plaintext.len());
    buf.extend_from_slice(key.as_slice());
    buf.extend_from_slice(nonce.as_slice());
    buf.extend_from_slice(plaintext);
    buf.into()
}

/// Encode input for AES-256-GCM decryption (0x67).
///
/// - `key`: 32-byte AES-256 key
/// - `nonce`: 12-byte nonce (96-bit, big-endian)
/// - `ciphertext_with_tag`: ciphertext + 16-byte GCM authentication tag
///
/// Format: `[key (32) | nonce (12) | ciphertext + tag...]`
pub fn encode_aes_decrypt(
    key: &FixedBytes<32>,
    nonce: &FixedBytes<12>,
    ciphertext_with_tag: &[u8],
) -> Bytes {
    let mut buf = Vec::with_capacity(44 + ciphertext_with_tag.len());
    buf.extend_from_slice(key.as_slice());
    buf.extend_from_slice(nonce.as_slice());
    buf.extend_from_slice(ciphertext_with_tag);
    buf.into()
}

/// Encode input for the HKDF precompile (0x68).
///
/// - `ikm`: input key material (arbitrary bytes)
///
/// Uses HKDF-SHA256 with no salt and info label `"seismic_hkdf_105"`.
/// Returns a 32-byte AES-256 key.
pub fn encode_hkdf(ikm: &[u8]) -> Bytes {
    ikm.to_vec().into()
}

/// Encode input for the SECP256K1 signing precompile (0x69).
///
/// - `secret_key`: 32-byte secp256k1 secret key
/// - `message_hash`: 32-byte message digest (typically keccak256)
///
/// Format: `[secret_key (32) | message_hash (32)]` → 64 bytes total
pub fn encode_secp256k1_sign(
    secret_key: &FixedBytes<32>,
    message_hash: &FixedBytes<32>,
) -> Bytes {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(secret_key.as_slice());
    buf.extend_from_slice(message_hash.as_slice());
    buf.into()
}

// ---------------------------------------------------------------------------
// Output decoding
// ---------------------------------------------------------------------------

/// Decoded output from the SECP256K1 signing precompile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverableSignature {
    /// 64-byte compact ECDSA signature (r || s).
    pub signature: FixedBytes<64>,
    /// Recovery ID (0–3).
    pub recovery_id: u8,
}

/// Decode the 65-byte output from the SECP256K1 signing precompile (0x69).
///
/// Returns `None` if the output is not exactly 65 bytes.
pub fn decode_secp256k1_sign(output: &[u8]) -> Option<RecoverableSignature> {
    if output.len() != 65 {
        return None;
    }
    Some(RecoverableSignature {
        signature: FixedBytes::from_slice(&output[..64]),
        recovery_id: output[64],
    })
}

// ---------------------------------------------------------------------------
// Convenience call wrappers
// ---------------------------------------------------------------------------

/// Convenience wrappers that encode inputs, call the precompile via `eth_call`,
/// and return decoded outputs.
///
/// These are useful for testing and off-chain verification. On-chain code
/// should call the precompile addresses directly from Solidity.
pub mod call {
    use alloy_network::TransactionBuilder;
    use alloy_provider::Provider;
    use alloy_transport::TransportResult;
    use seismic_alloy_network::seismic_network::SeismicNetwork;
    use seismic_alloy_rpc_types::SeismicTransactionRequest;

    use super::*;

    /// Helper: build and execute an `eth_call` to a precompile address.
    async fn precompile_call<N, P>(
        provider: &P,
        address: Address,
        input: Bytes,
    ) -> TransportResult<Bytes>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        let mut tx: N::TransactionRequest =
            SeismicTransactionRequest::default().to(address).into();
        TransactionBuilder::<N>::set_input(&mut tx, input);
        provider.call(tx).await
    }

    /// Generate random bytes via the RNG precompile (0x64).
    ///
    /// - `output_len`: number of random bytes
    /// - `personalization`: domain separation (must be non-empty)
    pub async fn rng<N, P>(
        provider: &P,
        output_len: u32,
        personalization: &[u8],
    ) -> TransportResult<Bytes>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        precompile_call::<N, P>(
            provider,
            addresses::RNG,
            encode_rng(output_len, personalization),
        )
        .await
    }

    /// Derive a 32-byte AES key via ECDH (0x65).
    pub async fn ecdh<N, P>(
        provider: &P,
        secret_key: &FixedBytes<32>,
        compressed_pubkey: &[u8; 33],
    ) -> TransportResult<FixedBytes<32>>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        let output = precompile_call::<N, P>(
            provider,
            addresses::ECDH,
            encode_ecdh(secret_key, compressed_pubkey),
        )
        .await?;
        Ok(FixedBytes::from_slice(&output))
    }

    /// Encrypt data via AES-256-GCM (0x66). Returns ciphertext + 16-byte tag.
    pub async fn aes_encrypt<N, P>(
        provider: &P,
        key: &FixedBytes<32>,
        nonce: &FixedBytes<12>,
        plaintext: &[u8],
    ) -> TransportResult<Bytes>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        precompile_call::<N, P>(
            provider,
            addresses::AES_ENCRYPT,
            encode_aes_encrypt(key, nonce, plaintext),
        )
        .await
    }

    /// Decrypt data via AES-256-GCM (0x67). Returns plaintext.
    pub async fn aes_decrypt<N, P>(
        provider: &P,
        key: &FixedBytes<32>,
        nonce: &FixedBytes<12>,
        ciphertext_with_tag: &[u8],
    ) -> TransportResult<Bytes>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        precompile_call::<N, P>(
            provider,
            addresses::AES_DECRYPT,
            encode_aes_decrypt(key, nonce, ciphertext_with_tag),
        )
        .await
    }

    /// Derive a 32-byte AES key via HKDF-SHA256 (0x68).
    pub async fn hkdf<N, P>(
        provider: &P,
        ikm: &[u8],
    ) -> TransportResult<FixedBytes<32>>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        let output = precompile_call::<N, P>(
            provider,
            addresses::HKDF,
            encode_hkdf(ikm),
        )
        .await?;
        Ok(FixedBytes::from_slice(&output))
    }

    /// Sign a message digest via SECP256K1 (0x69). Returns a recoverable signature.
    pub async fn secp256k1_sign<N, P>(
        provider: &P,
        secret_key: &FixedBytes<32>,
        message_hash: &FixedBytes<32>,
    ) -> TransportResult<RecoverableSignature>
    where
        N: SeismicNetwork,
        N::TransactionRequest: From<SeismicTransactionRequest>,
        N::UnsignedTx: Send + Sync,
        P: Provider<N>,
    {
        let output = precompile_call::<N, P>(
            provider,
            addresses::SECP256K1_SIGN,
            encode_secp256k1_sign(secret_key, message_hash),
        )
        .await?;
        decode_secp256k1_sign(&output).ok_or_else(|| {
            crate::SeismicProviderError::PrecompileOutput(format!(
                "SECP256K1 sign: expected 65-byte output, got {} bytes",
                output.len()
            ))
            .into_transport()
        })
    }
}
