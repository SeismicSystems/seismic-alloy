//! Seismic provider for websocket requests
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use seismic_alloy_network::seismic_network::SeismicNetwork;

use crate::SeismicProviderExt;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_utils::{ContractTestContext, ISeismicCounter},
        SeismicSignedProvider,
    };
    use alloy_network::{ReceiptResponse, TransactionBuilder};
    use alloy_node_bindings::{Anvil, AnvilInstance};
    use alloy_primitives::TxKind;
    use alloy_rpc_types_eth::Filter;
    use alloy_signer_local::PrivateKeySigner;
    use alloy_sol_types::SolEvent;
    use futures_util::StreamExt;
    use seismic_alloy_network::{
        foundry::{builder::seismic_foundry_tx_builder, SeismicFoundry},
        wallet::SeismicWallet,
    };
    use std::time::Duration;
    const SANVIL_PATH: &str = "sanvil";
    use seismic_alloy_consensus::{TxSeismic, TxSeismicElements};

    #[tokio::test]
    async fn test_subscribe_to_events() {
        let plaintext = ContractTestContext::get_deploy_input_plaintext();
        let anvil = Anvil::at(SANVIL_PATH).port(8545 as u16).block_time(2).spawn();
        let wallet = get_wallet(&anvil);
        let provider = SeismicSignedProvider::<SeismicFoundry>::new(wallet, anvil.endpoint_url());
        let ws_provider =
            SeismicUnsignedWsProvider::<SeismicFoundry>::new(anvil.ws_endpoint()).await.unwrap();
        let tx =
            seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();
        let contract_address = receipt.contract_address.unwrap();
        let code = provider.get_code_at(contract_address).await.unwrap();
        assert_eq!(code, ContractTestContext::get_code());
        let filter = Filter::new().address(contract_address);

        let event_sub = ws_provider.inner().subscribe_logs(&filter).await.unwrap();

        let network_pk = provider.get_tee_pubkey().await.unwrap();
        let encryption_keypair = TxSeismicElements::get_rand_encryption_keypair();
        let elements = TxSeismicElements::default()
            .with_encryption_pubkey(encryption_keypair.public_key())
            .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce());

        let tx_input = ContractTestContext::get_set_number_input_plaintext();
        let encrypted_input = elements
            .client_encrypt(&tx_input, &network_pk, &encryption_keypair.secret_key())
            .unwrap();

        let mut tx = seismic_foundry_tx_builder()
            .with_input(encrypted_input)
            .with_kind(TxKind::Call(contract_address))
            .into();
        tx.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        tx.seismic_elements = Some(elements);

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();

        assert!(receipt.status());

        let tx_input = ContractTestContext::get_increment_input_plaintext();
        let encrypted_input = elements
            .client_encrypt(&tx_input, &network_pk, &encryption_keypair.secret_key())
            .unwrap();

        let mut tx = seismic_foundry_tx_builder()
            .with_input(encrypted_input)
            .with_kind(TxKind::Call(contract_address))
            .into();
        tx.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        tx.seismic_elements = Some(elements);

        let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
        let receipt = pending_tx.get_receipt().await.unwrap();

        assert!(receipt.status());

        // Check for events with timeout
        let mut event_stream = event_sub.into_stream();
        let mut num_set_events_received = 0;
        let mut num_increment_events_received = 0;
        let mut total_events_received = 0;

        for _ in 0..5 {
            tokio::select! {
                event_opt = event_stream.next() => {
                    match event_opt {
                        Some(log) => {
                            if log.topic0() == Some(&ISeismicCounter::setNumberEmit::SIGNATURE_HASH) {
                                println!("NumberSet event received: {:?}", log);
                                num_set_events_received += 1;
                                total_events_received += 1;
                            }
                            else if log.topic0() == Some(&ISeismicCounter::incrementEmit::SIGNATURE_HASH) {
                                println!("Increment event received: {:?}", log);
                                num_increment_events_received += 1;
                                total_events_received += 1;
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

        assert!(
            num_set_events_received == 1,
            "Number of set events received: {}",
            num_set_events_received
        );
        assert!(
            num_increment_events_received == 1,
            "Number of increment events received: {}",
            num_increment_events_received
        );
        assert!(total_events_received == 2, "Total events received: {}", total_events_received);

        drop(anvil);
    }

    fn get_wallet(anvil: &AnvilInstance) -> SeismicWallet<SeismicFoundry> {
        let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
        let wallet = SeismicWallet::from(bob.clone());
        wallet
    }
}
