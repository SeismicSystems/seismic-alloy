//! Custom error types for the Seismic provider.
//!
//! Provides [`SeismicProviderError`] — a structured enum replacing ad-hoc
//! `TransportErrorKind::custom_str(...)` calls throughout the provider crate.

use alloy_transport::TransportErrorKind;

/// Errors specific to Seismic provider operations.
#[derive(Debug, thiserror::Error)]
pub enum SeismicProviderError {
    /// ABI decoding of a contract call return value failed.
    #[error("ABI decode error: {0}")]
    AbiDecode(alloy_sol_types::Error),

    /// Response decryption failed (ECDH / AEAD).
    #[error("response decryption failed: {0}")]
    Decryption(String),

    /// Sender address is missing from a filled transaction that requires it
    /// for metadata extraction (e.g., decryption).
    #[error("sender address required for seismic call decryption")]
    MissingSender,

    /// Failed to construct seismic metadata from a filled transaction.
    #[error("error creating seismic metadata: {0}")]
    MetadataCreation(String),

    /// Expected a seismic envelope for decryption but received a non-seismic one.
    #[error("expected seismic envelope for decryption")]
    NotSeismicEnvelope,

    /// EIP-712 send: the filled transaction is not an EIP-712 seismic envelope.
    #[error("EIP-712 send: filled transaction is not an EIP-712 seismic envelope")]
    Eip712NotSeismicEnvelope,

    /// EIP-712 send: expected a signed envelope after filling but got a builder.
    #[error("EIP-712 send: expected signed envelope after filling, got builder")]
    Eip712GotBuilder,

    /// Precompile returned output in an unexpected format.
    #[error("precompile output error: {0}")]
    PrecompileOutput(String),
}

impl SeismicProviderError {
    /// Convert into an [`alloy_transport::TransportError`] for propagation.
    pub fn into_transport(self) -> alloy_transport::TransportError {
        TransportErrorKind::custom(self)
    }
}
