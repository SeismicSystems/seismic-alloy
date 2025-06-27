//! Seismic provider for websocket requests
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use seismic_alloy_network::seismic_network::SeismicNetwork;
use crate::SeismicProviderExt;
use std::time::Duration;

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
        let provider = ProviderBuilder::new_with_network::<N>()
            .connect(&url.into())
            .await?
            .root()
            .clone();
        Ok(Self { provider })
    }

    /// Get the inner provider
    pub fn inner(&self) -> &SeismicUnsignedWsProviderInner<N> {
        &self.provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{provider::SeismicUnsignedProvider, test_utils::ContractTestContext};
    use alloy_network::{ReceiptResponse, TransactionBuilder};
    use alloy_node_bindings::{Anvil, AnvilInstance};
    use alloy_primitives::{address, Address, Bytes, TxKind};
    use alloy_signer_local::PrivateKeySigner;
    use seismic_alloy_network::{
        foundry::{builder::seismic_foundry_tx_builder, SeismicFoundry},
        wallet::SeismicWallet,
    };
    use alloy_pubsub::Subscription;
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use alloy_rpc_types_eth::Filter;
    const SANVIL_PATH: &str = "sanvil";
    

    #[tokio::test]
    async fn test_subscribe_to_events() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).port(8545 as u16).block_time(2).spawn();
        let from = get_wallet(&anvil).default_signer().address();
        let provider = SeismicUnsignedProvider::<SeismicFoundry>::new(anvil.endpoint_url());
        let ws_provider = SeismicUnsignedWsProvider::<SeismicFoundry>::new("ws://localhost:8545").await.unwrap();
        let tx =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();
        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());
        let filter = Filter::new().address(contract_address);

        let mut event_sub = ws_provider.inner().subscribe_logs(&filter).await.unwrap();

        // Trigger some events by calling contract functions
        let increment_tx = seismic_foundry_tx_builder()
            .with_input(ContractTestContext::get_increment_input_plaintext())
            .with_kind(TxKind::Call(contract_address))
            .into();
        
        provider.send_transaction(increment_tx.into()).await.unwrap().get_receipt().await.unwrap();

        let set_number_tx = seismic_foundry_tx_builder()
            .with_input(ContractTestContext::get_set_number_input_plaintext())
            .with_kind(TxKind::Call(contract_address))
            .into();
        
        provider.send_transaction(set_number_tx.into()).await.unwrap().get_receipt().await.unwrap();

        // Check for events with timeout
        let mut event_stream = event_sub.into_stream();
        let mut events_received = 0;

        for _ in 0..2 {
            tokio::select! {
                event_opt = event_stream.next() => {
                    match event_opt {
                        Some(log) => {
                            if (log.topic0() == Some(&Bytes::from_static(b"NumberSet(uint256)"))) {
                                println!("NumberSet event received: {:?}", log);
                                events_received += 1;
                            }
                        }
                        None => {
                            println!("Event stream ended");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    break;
                }
            }
        }

        assert!(events_received > 0, "No events received");
    }

    fn get_wallet(anvil: &AnvilInstance) -> SeismicWallet<SeismicFoundry> {
        let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
        let wallet = SeismicWallet::from(bob.clone());
        wallet
    }
}
