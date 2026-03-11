//! Seismic provider
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod traits;
pub use traits::SeismicProviderExt;

mod call_ext;
pub use call_ext::{SeismicCallExt, SeismicSolCallBuilder};

pub mod decrypt;
pub use decrypt::ResponseDecryptProvider;

pub mod builder;
pub use builder::{
    SeismicSignedProvider, SeismicUnsignedProvider, sfoundry_signed_provider,
    sfoundry_unsigned_provider, signed_provider, signed_provider_with_tee_pubkey,
    sreth_signed_provider, sreth_unsigned_provider, unsigned_provider_http,
    unsigned_provider_ws,
};

pub mod test_utils;

#[cfg(test)]
mod tests;
