//! Seismic provider
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod traits;
pub use traits::SeismicProviderExt;

mod call_ext;
pub use call_ext::{SeismicCallExt, SeismicSolCallBuilder};

pub mod provider;
pub use provider::{SeismicSignedProvider, SeismicUnsignedProvider};

pub mod test_utils;
