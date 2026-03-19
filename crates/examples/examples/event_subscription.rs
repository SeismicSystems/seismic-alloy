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

use alloy_network::ReceiptResponse;
use alloy_node_bindings::Anvil;
use alloy_primitives::U256;
use alloy_provider::Provider;
use alloy_rpc_types_eth::Filter;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolEvent};
use futures_util::StreamExt;
use seismic_alloy_network::{foundry::SeismicFoundry, wallet::SeismicWallet};
use seismic_alloy_provider::{SeismicCallExt, SeismicProviderBuilder, ShieldedCallExt};

// See basic_contract.rs for the Solidity source.
sol! {
    #[sol(rpc, bytecode = "60806040525f5f8190b1506102e2806100175f395ff3fe608060405234801561000f575f5ffd5b506004361061003f575f3560e01c806324a7f0b71461004357806343bd0d701461005f578063d09de08a1461007d575b5f5ffd5b61005d6004803603810190610058919061014e565b610087565b005b6100676100bc565b6040516100749190610193565b60405180910390f35b6100856100d3565b005b805f8190b1507fd5d7fa14c63c3a6cb5e6dd4b4bb8c48d371a807bd306e9c09f1d61769963402c60405160405180910390a150565b5f600160025fb06100cd91906101e2565b14905090565b5f5f81b0809291906100e49061023f565b919050b1507f9ff5ccac5db99a217f56663c2490d2cb74f1512ec2f298bb1c8b7ffc56dae36e60405160405180910390a1565b5f5ffd5b5f819050919050565b61012d8161011b565b8114610137575f5ffd5b50565b5f8135905061014881610124565b92915050565b5f6020828403121561016357610162610117565b5b5f6101708482850161013a565b91505092915050565b5f8115159050919050565b61018d81610179565b82525050565b5f6020820190506101a65f830184610184565b92915050565b5f819050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601260045260245ffd5b5f6101ec826101ac565b91506101f7836101ac565b925082610207576102066101b5565b5b828206905092915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f6102498261011b565b91507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff820361027b5761027a610212565b5b60018201905091905056fea264697066735822122005eb17b23e331c07f3f13292a557c76fafce3a9e7ac80b829875dffc0aece52664736f6c637828302e382e32382d646576656c6f702e323032352e332e31332b636f6d6d69742e64306231386234650059")]
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
    let ws_provider =
        SeismicProviderBuilder::new().foundry().connect_ws(anvil.ws_endpoint_url()).await?;

    // Deploy contract
    let contract = SeismicCounter::deploy(&provider).await?;
    let addr = *contract.address();
    println!("Contract deployed at {addr}");

    // Subscribe to all events from this contract.
    let filter = Filter::new().address(addr);
    let sub = ws_provider.subscribe_logs(&filter).await?;
    let mut stream = sub.into_stream();
    println!("Subscribed to events from {addr}");

    // Send some transactions that emit events.
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(42)))
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("setNumber(42) tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    let receipt = contract.increment().seismic().send().await?.get_receipt().await?;
    println!("increment() tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

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
