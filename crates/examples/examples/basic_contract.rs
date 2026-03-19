//! Basic contract interaction with seismic-alloy.
//!
//! Demonstrates:
//! - Building a signed provider
//! - Deploying a contract via the `sol!` macro's generated `deploy()`
//! - Shielded (encrypted) reads and writes — auto-encrypted for functions with
//!   shielded params, or via `.seismic()` for non-shielded functions
//! - Transparent (unencrypted) reads and writes
//!
//! # Running
//!
//! Requires `sanvil` in `$PATH` or `$HOME/.seismic/bin/`.
//!
//! ```sh
//! cargo run -p seismic-examples --example basic_contract
//! ```

use alloy_network::ReceiptResponse;
use alloy_node_bindings::Anvil;
use alloy_primitives::U256;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use seismic_alloy_network::{foundry::SeismicFoundry, wallet::SeismicWallet};
use seismic_alloy_provider::{SeismicCallExt, SeismicProviderBuilder, ShieldedCallExt};

// Solidity source (compiled with seismic solc):
//
//   pragma solidity ^0.8.13;
//   contract SeismicCounter {
//       suint256 number;
//       event setNumberEmit();
//       event incrementEmit();
//       constructor() payable { number = suint256(0); }
//       function setNumber(suint256 newNumber) public {
//           number = newNumber;
//           emit setNumberEmit();
//       }
//       function increment() public {
//           number++;
//           emit incrementEmit();
//       }
//       function isOdd() public view returns (bool) {
//           return uint256(number) % 2 == 1;
//       }
//   }
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
    let contract = SeismicCounter::deploy(&provider).await?;
    let addr = *contract.address();
    println!("Contract deployed at {addr}");

    // 3. Shielded read: isOdd() — number is 0, so false.
    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: false)");
    assert!(!is_odd);

    // 4. Shielded write: setNumber(7) — auto-encrypts because suint256 is shielded.
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(U256::from(7)))
        .send()
        .await?
        .get_receipt()
        .await?;
    println!("setNumber(7) tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    // 5. Shielded read: isOdd() — 7 is odd, so true.
    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: true)");
    assert!(is_odd);

    // 6. Transparent read (no encryption, standard eth_call).
    let is_odd = contract.isOdd().call().await?;
    println!("isOdd() [transparent] = {is_odd}");

    // 7. Transparent write: increment()
    let receipt = contract.increment().send().await?.get_receipt().await?;
    println!("increment() tx: {:?} (status: {})", receipt.transaction_hash, receipt.status());

    // 8. Shielded read after increment: 8 is even.
    let is_odd = contract.isOdd().seismic().call().await?;
    println!("isOdd() = {is_odd} (expected: false)");
    assert!(!is_odd);

    // 9. Also works with the low-level trait methods.
    use seismic_alloy_provider::SeismicProviderExt;

    let result = provider.shielded_call(addr, SeismicCounter::isOddCall {}).await?;
    println!("shielded_call isOdd() = {result}");

    let tee_pubkey = provider.get_tee_pubkey().await?;
    println!("TEE public key: {tee_pubkey}");

    println!("\nAll operations completed successfully!");
    Ok(())
}
