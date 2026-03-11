//! WebSocket event subscription.
//!
//! Demonstrates:
//! - Creating an unsigned WebSocket provider for event listening
//! - Subscribing to contract events with filters
//! - Processing events from a stream
//!
//! # Running
//!
//! Requires `sanvil` in `$PATH` or `$HOME/.seismic/bin/`.
//!
//! ```sh
//! cargo run -p seismic-examples --example event_subscription
//! ```

use alloy_network::TransactionBuilder;
use alloy_node_bindings::Anvil;
use alloy_primitives::{Bytes, TxKind, U256};
use alloy_provider::Provider;
use alloy_rpc_types_eth::Filter;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolEvent};
use futures_util::StreamExt;
use seismic_alloy_network::{
    foundry::{builder::seismic_foundry_tx_builder, SeismicFoundry},
    wallet::SeismicWallet,
};
use seismic_alloy_provider::{SeismicCallExt, SeismicProviderBuilder};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

sol! {
    #[sol(rpc)]
    contract SeismicCounter {
        event setNumberEmit();
        event incrementEmit();
        function setNumber(suint256 newNumber) public;
        function increment() public;
        function isOdd() public view returns (bool);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use block_time(2) so events get mined predictably.
    let anvil = Anvil::at("sanvil").block_time(2).spawn();
    let signer: PrivateKeySigner = anvil.keys()[1].clone().into();
    let wallet = SeismicWallet::<SeismicFoundry>::from(signer);

    // Signed HTTP provider for sending transactions.
    let provider = SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await?;

    // Unsigned WS provider for subscribing to events.
    let ws_provider = SeismicProviderBuilder::new()
        .foundry()
        .connect_ws(anvil.ws_endpoint_url())
        .await?;

    // Deploy contract
    let bytecode = Bytes::from_static(include_bytes!("../bytecode/seismic_counter.bin"));
    let deploy_tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(bytecode)
        .with_kind(TxKind::Create)
        .into();

    let receipt = provider
        .send_transaction(deploy_tx.into())
        .await?
        .get_receipt()
        .await?;
    let addr = receipt.contract_address.unwrap();
    println!("Contract deployed at {addr}");

    let contract = SeismicCounter::new(addr, &provider);

    // Subscribe to all events from this contract.
    let filter = Filter::new().address(addr);
    let sub = ws_provider.subscribe_logs(&filter).await?;
    let mut stream = sub.into_stream();
    println!("Subscribed to events from {addr}");

    // Send some transactions that emit events.
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(42)))
        .seismic()
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("setNumber(42) tx: {:?}", receipt.transaction_hash);

    let receipt = contract
        .increment()
        .seismic()
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("increment() tx: {:?}", receipt.transaction_hash);

    // Collect events.
    let mut set_events = 0u32;
    let mut increment_events = 0u32;

    println!("\nListening for events...");
    loop {
        tokio::select! {
            Some(log) = stream.next() => {
                if log.topic0() == Some(&SeismicCounter::setNumberEmit::SIGNATURE_HASH) {
                    set_events += 1;
                    println!("  setNumberEmit (block {})", log.block_number.unwrap_or(0));
                } else if log.topic0() == Some(&SeismicCounter::incrementEmit::SIGNATURE_HASH) {
                    increment_events += 1;
                    println!("  incrementEmit (block {})", log.block_number.unwrap_or(0));
                }

                if set_events + increment_events >= 2 {
                    break;
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                println!("Timeout waiting for events");
                break;
            }
        }
    }

    println!("\nReceived {set_events} setNumberEmit + {increment_events} incrementEmit events");
    assert_eq!(set_events, 1);
    assert_eq!(increment_events, 1);

    println!("Event subscription example completed successfully!");
    Ok(())
}
