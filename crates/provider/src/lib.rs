//! Seismic provider
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod traits;
pub use traits::SeismicProviderExt;

pub mod provider;
pub use provider::{SeismicSignedProvider, SeismicUnsignedProvider};

pub mod test_utils;
