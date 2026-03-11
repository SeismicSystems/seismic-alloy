//! Basic contract interaction with seismic-alloy.
//!
//! Demonstrates:
//! - Building a signed provider
//! - Deploying a contract
//! - Shielded (encrypted) reads and writes via `sol!` + `.seismic()`
//! - Transparent (unencrypted) reads and writes
//!
//! # Running
//!
//! Requires `sanvil` in `$PATH` or `$HOME/.seismic/bin/`.
//!
//! ```sh
//! cargo run -p seismic-examples --example basic_contract
//! ```

use alloy_network::{ReceiptResponse, TransactionBuilder};
use alloy_node_bindings::Anvil;
use alloy_primitives::{Bytes, TxKind, U256};
use alloy_provider::Provider;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use seismic_alloy_network::{
    foundry::{builder::seismic_foundry_tx_builder, SeismicFoundry},
    wallet::SeismicWallet,
};
use seismic_alloy_provider::{SeismicCallExt, SeismicProviderBuilder};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

// Define the contract interface using the sol! macro.
// `#[sol(rpc)]` generates typed call builders for each function.
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
    // 1. Spawn a local sanvil instance and create a signed provider.
    let anvil = Anvil::at("sanvil").spawn();
    let signer: PrivateKeySigner = anvil.keys()[1].clone().into();
    let wallet = SeismicWallet::<SeismicFoundry>::from(signer);

    let provider = SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await?;

    println!("Connected to sanvil at {}", anvil.endpoint());

    // 2. Deploy the SeismicCounter contract.
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
    let contract_addr = receipt.contract_address.expect("deployment should return address");
    println!("Contract deployed at {contract_addr}");

    // 3. Create a typed contract handle.
    let contract = SeismicCounter::new(contract_addr, &provider);

    // 4. Shielded read: isOdd() — number is 0, so false.
    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: false)");
    assert!(!is_odd);

    // 5. Shielded write: setNumber(7)
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(7)))
        .seismic()
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("setNumber(7) tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    // 6. Shielded read: isOdd() — 7 is odd, so true.
    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: true)");
    assert!(is_odd);

    // 7. Transparent read (no encryption, standard eth_call).
    let is_odd = contract.isOdd().call().await?;
    println!("isOdd() [transparent] = {is_odd}");

    // 8. Transparent write: increment()
    let receipt = contract
        .increment()
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("increment() tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    // 9. Shielded read after increment: 8 is even.
    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: false)");
    assert!(!is_odd);

    // 10. Also works with the low-level trait methods.
    use seismic_alloy_provider::SeismicProviderExt;

    let result = provider
        .shielded_call(contract_addr, SeismicCounter::isOddCall {})
        .await?;
    println!("shielded_call isOdd() = {result}");

    let tee_pubkey = provider.get_tee_pubkey().await?;
    println!("TEE public key: {tee_pubkey}");

    println!("\nAll operations completed successfully!");
    Ok(())
}
