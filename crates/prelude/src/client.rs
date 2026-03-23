//! User-facing prelude for the seismic-alloy SDK.
//!
//! ```rust,ignore
//! use seismic_prelude::client::*;
//! ```
//!
//! This module re-exports the types and traits needed for common
//! seismic-alloy usage: provider setup, contract interaction, wallet
//! creation, and the `sol!` macro.

// Seismic provider & call traits
pub use seismic_alloy_provider::{
    SecurityParams, SeismicCallExt, SeismicProviderBuilder, SeismicProviderExt,
    SeismicSignedProvider, SeismicUnsignedProvider, ShieldedCallExt, SignedProviderExt,
};

// Wallet
pub use seismic_alloy_network::wallet::SeismicWallet;

// sol! macro
pub use alloy_sol_types::sol;

// Common primitives
pub use alloy_primitives::{Address, Bytes, FixedBytes, U256};

// Common alloy traits
pub use alloy_network::ReceiptResponse;

// Signer
pub use alloy_signer_local::PrivateKeySigner;
