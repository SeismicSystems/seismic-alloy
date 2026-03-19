//! Per-call security parameter customization.
//!
//! Demonstrates overriding the default security parameters on a per-call
//! basis using the `.seismic()` builder methods:
//!
//! - `.expires_at(block)` — set transaction expiration block
//! - `.encryption_nonce(nonce)` — set a custom AEAD nonce (testing only)
//! - `.recent_block_hash(hash)` — pin to a specific chain state
//!
//! # Running
//!
//! Requires `sanvil` in `$PATH` or `$HOME/.seismic/bin/`.
//!
//! ```sh
//! cargo run -p seismic-examples --example security_params
//! ```

use alloy_network::ReceiptResponse;
use alloy_node_bindings::Anvil;
use alloy_primitives::{aliases::U96, U256};
use alloy_provider::Provider;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
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
    let anvil = Anvil::at("sanvil").spawn();
    let signer: PrivateKeySigner = anvil.keys()[1].clone().into();
    let wallet = SeismicWallet::<SeismicFoundry>::from(signer);

    let provider = SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await?;

    let contract = SeismicCounter::deploy(&provider).await?;
    println!("Contract deployed at {}", contract.address());

    let current_block = provider.get_block_number().await?;
    println!("Current block: {current_block}");

    // ---- Custom expiration ----
    // By default, transactions expire at current_block + 100.
    // Here we set a shorter window.
    let is_odd = contract.isOdd().seismic().expires_at(current_block + 10).call().await?;
    println!("isOdd() with expires_at={} = {is_odd}", current_block + 10);

    // ---- Custom encryption nonce ----
    // By default, a random nonce is generated for each call.
    // A fixed nonce is useful for deterministic testing.
    // WARNING: Never reuse nonces in production — it breaks encryption.
    let custom_nonce = U96::from(0xDEADBEEFu64);
    let is_odd = contract.isOdd().seismic().encryption_nonce(custom_nonce).call().await?;
    println!("isOdd() with custom nonce = {is_odd}");

    // ---- Custom recent_block_hash ----
    // By default, the filler fetches the latest block hash.
    // Providing it manually skips one RPC round-trip.
    let block =
        provider.get_block_by_number(alloy_rpc_types_eth::BlockNumberOrTag::Latest).await?.unwrap();
    let block_hash = block.header.hash;
    println!("Pinning to block hash: {block_hash}");

    let is_odd = contract
        .isOdd()
        .seismic()
        .recent_block_hash(block_hash)
        .expires_at(current_block + 50)
        .call()
        .await?;
    println!("isOdd() with pinned block hash = {is_odd}");

    // ---- Security params on writes ----
    // setNumber auto-encrypts since suint256 is shielded.
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(3)))
        .expires_at(current_block + 50)
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("setNumber(3) with expires_at={} (status: {})", current_block + 50, receipt.status());

    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: true)");
    assert!(is_odd);

    println!("\nSecurity params example completed successfully!");
    Ok(())
}
