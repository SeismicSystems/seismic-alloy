//! Seismic provider
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod security_params;
pub use security_params::SecurityParams;

mod traits;
pub use traits::{SeismicProviderExt, SignedProviderExt};

mod call_ext;
pub use call_ext::{SeismicCallExt, ShieldedCallExt};

pub mod error;
pub use error::SeismicProviderError;

pub mod decrypt;
pub use decrypt::ResponseDecryptProvider;

pub mod builder;
pub use builder::{SeismicProviderBuilder, SeismicSignedProvider, SeismicUnsignedProvider};

pub mod precompiles;

pub mod test_utils;

#[cfg(test)]
mod tests;
