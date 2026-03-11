//! EIP-712 typed data signing for Seismic transactions.
//!
//! Demonstrates using `.eip712()` to sign transactions via EIP-712
//! `signTypedData` instead of standard RLP signing. This is primarily
//! needed for browser wallet (e.g., MetaMask) integration, where the
//! wallet can't sign custom RLP-encoded transaction types.
//!
//! # Running
//!
//! Requires `sanvil` in `$PATH` or `$HOME/.seismic/bin/`.
//!
//! ```sh
//! cargo run -p seismic-examples --example eip712
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

    // ---- EIP-712 signed write ----
    // `.seismic().eip712()` sets message_version = 2, which makes the
    // transaction get signed via EIP-712 signTypedData and sent as a
    // TypedDataRequest instead of raw bytes.
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(13)))
        .seismic()
        .eip712()
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("EIP-712 setNumber(13) tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    // ---- EIP-712 signed read ----
    let is_odd = contract.isOdd().seismic().eip712().call().await?;
    println!("EIP-712 isOdd() = {is_odd} (expected: true)");
    assert!(is_odd);

    // ---- Standard signed write (for comparison) ----
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(4)))
        .seismic()
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("Standard setNumber(4) tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    // ---- EIP-712 read after standard write ----
    // EIP-712 and standard signing are interchangeable for reads.
    let is_odd = contract.isOdd().seismic().eip712().call().await?;
    println!("EIP-712 isOdd() = {is_odd} (expected: false)");
    assert!(!is_odd);

    println!("\nEIP-712 example completed successfully!");
    Ok(())
}
