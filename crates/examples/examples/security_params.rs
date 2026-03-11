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

use alloy_network::{ReceiptResponse, TransactionBuilder};
use alloy_node_bindings::Anvil;
use alloy_primitives::{aliases::U96, Bytes, TxKind, U256};
use alloy_provider::Provider;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use seismic_alloy_network::{
    foundry::{builder::seismic_foundry_tx_builder, SeismicFoundry},
    wallet::SeismicWallet,
};
use seismic_alloy_provider::{SeismicCallExt, SeismicProviderBuilder};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

sol! {
    #[sol(rpc)]
    contract SeismicCounter {
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
    let current_block = provider.get_block_number().await?;
    println!("Current block: {current_block}");

    // ---- Custom expiration ----
    // By default, transactions expire at current_block + 100.
    // Here we set a shorter window.
    let is_odd = contract
        .isOdd()
        .seismic()
        .expires_at(current_block + 10)
        .call()
        .await?;
    println!("isOdd() with expires_at={} = {is_odd}", current_block + 10);

    // ---- Custom encryption nonce ----
    // By default, a random nonce is generated for each call.
    // A fixed nonce is useful for deterministic testing.
    // WARNING: Never reuse nonces in production — it breaks encryption.
    let custom_nonce = U96::from(0xDEADBEEFu64);
    let is_odd = contract
        .isOdd()
        .seismic()
        .encryption_nonce(custom_nonce)
        .call()
        .await?;
    println!("isOdd() with custom nonce = {is_odd}");

    // ---- Custom recent_block_hash ----
    // By default, the filler fetches the latest block hash.
    // Providing it manually skips one RPC round-trip.
    let block = provider
        .get_block_by_number(alloy_rpc_types_eth::BlockNumberOrTag::Latest)
        .await?
        .unwrap();
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
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(3)))
        .seismic()
        .expires_at(current_block + 50)
        .send()
        .await?
        .get_receipt()
        .await?;
    println!(
        "setNumber(3) with expires_at={} (status: {})",
        current_block + 50,
        receipt.status()
    );

    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: true)");
    assert!(is_odd);

    println!("\nSecurity params example completed successfully!");
    Ok(())
}
