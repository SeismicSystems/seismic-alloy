//! Seismic provider
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod traits;
pub use traits::SeismicProviderExt;

pub mod provider;
pub use provider::SeismicSignedProvider;

/// WebSocket provider implementation
pub mod ws_provider;
pub use ws_provider::SeismicUnsignedWsProvider;

pub mod test_utils;
