use crate::SeismicProviderExt;
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use seismic_alloy_network::seismic_network::SeismicNetwork;

/// Seismic unsigned websocket provider
pub type SeismicUnsignedWsProviderInner<N> = RootProvider<N>;

/// Seismic unsigned websocket provider
#[derive(Debug, Clone)]
pub struct SeismicUnsignedWsProvider<N: SeismicNetwork + Send + Sync>
where
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Inner provider
    pub provider: SeismicUnsignedWsProviderInner<N>,
}

impl<N: SeismicNetwork + Send + Sync> SeismicUnsignedWsProvider<N>
where
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// creates a new websocket provider for a client
    pub async fn new(url: impl Into<String>) -> Result<Self, alloy_transport::TransportError> {
        let provider =
            ProviderBuilder::new_with_network::<N>().connect(&url.into()).await?.root().clone();
        Ok(Self { provider })
    }

    /// Get the inner provider
    pub fn inner(&self) -> &SeismicUnsignedWsProviderInner<N> {
        &self.provider
    }
}
