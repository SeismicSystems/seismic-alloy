//! Custom fillers for the Seismic provider
//!
//! Normally fillers go in alloy-provider, but we need to put them here because
//! we need it to impl RecommendedFillers for the [`Seismic`] network.

use crate::seismic_network::SeismicNetwork;
use alloy_network::{Network, TransactionBuilder};
use alloy_provider::{
    fillers::{FillerControlFlow, GasFillable, TxFiller},
    Provider, SendableTx,
};
use alloy_transport::TransportResult;
use futures::FutureExt;
use seismic_alloy_consensus::InputDecryptionElements;
use std::future::IntoFuture;

pub use alloy_provider::fillers::GasFiller;

/// A wrapper for alloy_provider::fillers::GasFiller that handles gas for seismic transactions
/// Seismic tx need to be treated like a legacy tx
#[derive(Clone, Copy, Debug, Default)]
pub struct SeismicGasFiller(GasFiller);

impl SeismicGasFiller {
    /// copy and pasted from gas filler becuase it was private
    async fn seismic_prepare_legacy<P, N>(
        &self,
        provider: &P,
        tx: &N::TransactionRequest,
    ) -> TransportResult<GasFillable>
    where
        P: Provider<N>,
        N: SeismicNetwork,
        <N as Network>::UnsignedTx: Send + Sync,
    {
        let gas_price_fut = tx.gas_price().map_or_else(
            || provider.get_gas_price().right_future(),
            |gas_price| async move { Ok(gas_price) }.left_future(),
        );

        let gas_limit_fut = tx.gas_limit().map_or_else(
            || provider.estimate_gas(tx.clone()).into_future().right_future(),
            |gas_limit| async move { Ok(gas_limit) }.left_future(),
        );

        let (gas_price, gas_limit) = futures::try_join!(gas_price_fut, gas_limit_fut)?;

        Ok(GasFillable::Legacy { gas_limit, gas_price })
    }

    fn is_seismic_tx<N>(&self, tx: &N::TransactionRequest) -> bool
    where
        N: SeismicNetwork,
        N::TransactionRequest: InputDecryptionElements,
        <N as Network>::UnsignedTx: Send + Sync,
    {
        // TODO: it is probably more correct to check the tx type instead,
        // but we probably will get an error anyway if we have either combo of:
        // - a seismic tx with no decryption elements
        // - a non-seismic tx with decryption elements
        tracing::debug!(
            "SeismicGasFiller::is_seismic_tx. res: {:?}",
            N::is_seismic_tx_type(tx.output_tx_type()) || tx.get_decryption_elements().is_ok()
        );

        N::is_seismic_tx_type(tx.output_tx_type()) || tx.get_decryption_elements().is_ok()
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicGasFiller
where
    <N as Network>::TransactionRequest: InputDecryptionElements,
    <N as Network>::UnsignedTx: Send + Sync,
{
    type Fillable = GasFillable;

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        if self.is_seismic_tx::<N>(tx) {
            // tx is a seismic transaction, repeat logic for legacy tx
            if tx.gas_price().is_some() && tx.gas_limit().is_some() {
                return FillerControlFlow::Finished;
            } else {
                return FillerControlFlow::Ready;
            }
        } else {
            <GasFiller as TxFiller<N>>::status(&self.0, tx)
        }
    }

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {}

    async fn prepare<P>(
        &self,
        provider: &P,
        tx: &<N as Network>::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<N>,
    {
        if self.is_seismic_tx::<N>(tx) {
            // tx is a seismic transaction, repeat logic for legacy tx
            SeismicGasFiller::seismic_prepare_legacy(self, provider, tx).await
        } else {
            GasFiller::prepare(&self.0, provider, tx).await
        }
    }

    async fn fill(
        &self,
        fillable: Self::Fillable,
        tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        GasFiller::fill(&self.0, fillable, tx).await
    }
}
