//! Integration tests for the Seismic provider.
#![cfg(test)]
#![allow(deprecated)] // Tests exercise deprecated .seismic() on ShieldedCallBuilder

use crate::{
    builder::SeismicSignedProvider,
    test_utils::{ContractTestContext, ISeismicCounter},
    SeismicProviderExt, SignedProviderExt,
};
use alloy_network::{ReceiptResponse, TransactionBuilder};
use alloy_node_bindings::{Anvil, AnvilInstance};
use alloy_primitives::{address, hex, Address, Bytes, TxKind};
use alloy_provider::{ext::AnvilApi, Provider, SendableTx};
use alloy_rpc_types_eth::Filter;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolEvent};
use futures_util::StreamExt;
use seismic_alloy_consensus::SeismicReceiptEnvelope;
use seismic_alloy_network::{
    foundry::{builder::seismic_foundry_tx_builder, SeismicFoundry},
    wallet::SeismicWallet,
};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

/// Path to local sanvil binary for local testing
const SANVIL_PATH: &str = "sanvil";

#[tokio::test]
async fn test_get_tee_pubkey() {
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    let tee_pubkey = provider.get_tee_pubkey().await.unwrap();
    assert_eq!(tee_pubkey, seismic_enclave::get_unsecure_sample_secp256k1_pk());
}

#[tokio::test]
async fn test_send_transaction_with_empty_input() {
    let plaintext = Bytes::new();
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(plaintext).with_to(Address::ZERO).into();
    let res = provider.send_transaction(tx.into()).await.unwrap();
    let receipt = res.get_receipt().await.unwrap();
    assert_eq!(receipt.inner.inner.status(), true);
}

/// Check that SeismicUnsignedProvider correctly inherits alloy_provider ext traits
#[tokio::test]
async fn test_anvil_set_code() {
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    let address = address!("0xd8da6bf26964af9d7eed9e03e53415d37aa96045");
    provider.anvil_set_code(address, Bytes::from("0xbeef")).await.unwrap();

    let code = provider.get_code_at(address).await.unwrap();
    assert_eq!(code, Bytes::from("0xbeef"));
}

#[tokio::test]
async fn test_seismic_unsigned_call() {
    let plaintext = ContractTestContext::get_deploy_input_plaintext();
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let from = get_wallet(&anvil).default_signer().address();
    let unsigned_provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    // Make a regular (non-seismic) eth_call via standard Provider::call
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(plaintext)
        .with_kind(TxKind::Create)
        .with_from(from)
        .into();

    let res = unsigned_provider.call(tx.into()).await.unwrap();
    assert_eq!(res, ContractTestContext::get_code());
}

#[tokio::test]
async fn test_seismic_signed_call() {
    let plaintext = ContractTestContext::get_deploy_input_plaintext();
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Deploy contract with a regular (non-seismic) transaction
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

    let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
    let receipt = pending_tx.get_receipt().await.unwrap();
    let contract_address = receipt.contract_address.unwrap();

    // Now make a seismic call to the deployed contract using isOdd()
    let call_input = ContractTestContext::get_is_odd_input_plaintext();
    let tx = seismic_foundry_tx_builder()
        .with_input(call_input)
        .with_kind(TxKind::Call(contract_address))
        .into()
        .seismic();

    let res = provider.seismic_call_raw(SendableTx::Builder(tx.into())).await;
    assert!(res.is_ok(), "seismic_call failed: {:?}", res.unwrap_err());
    let res = res.unwrap();

    // Verify we got the expected result from isOdd() - number is 0 (even), so isOdd should
    // return false
    let expected = Bytes::from_static(&hex!(
        "0000000000000000000000000000000000000000000000000000000000000000"
    ));
    assert_eq!(res, expected);
}

#[tokio::test]
async fn test_send_transaction() {
    let plaintext = ContractTestContext::get_deploy_input_plaintext();
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Test sending a regular (non-seismic) Create transaction
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

    let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
    let receipt = pending_tx.get_receipt().await.unwrap();
    let contract_address = receipt.contract_address.unwrap();

    let code = provider.get_code_at(contract_address).await.unwrap();
    assert_eq!(code, ContractTestContext::get_code());
}

#[tokio::test]
async fn test_send_seismic_transaction() {
    let plaintext = ContractTestContext::get_deploy_input_plaintext();
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Deploy contract with a regular (non-seismic) transaction
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

    let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
    let receipt = pending_tx.get_receipt().await.unwrap();
    let contract_address = receipt.contract_address.unwrap();
    let code = provider.get_code_at(contract_address).await.unwrap();
    assert_eq!(code, ContractTestContext::get_code());

    // Use new .seismic() API - fillers handle encryption automatically
    let tx_input = ContractTestContext::get_set_number_input_plaintext();
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(tx_input)
        .with_kind(TxKind::Call(contract_address))
        .into()
        .seismic();

    let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
    let receipt = pending_tx.get_receipt().await.unwrap();

    assert!(receipt.status());
    match receipt.inner.inner {
        SeismicReceiptEnvelope::Seismic(_r) => {}
        _ => {
            panic!("expected seismic receipt");
        }
    }
}

#[tokio::test]
async fn test_subscribe_to_events() {
    let plaintext = ContractTestContext::get_deploy_input_plaintext();
    let anvil = Anvil::at(SANVIL_PATH).block_time(2).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();
    let ws_provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .connect_ws(anvil.ws_endpoint_url())
        .await
        .unwrap();

    // Deploy contract with a regular (non-seismic) transaction
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();
    let pending_tx = provider.send_transaction(tx.into()).await.unwrap();
    let receipt = pending_tx.get_receipt().await.unwrap();
    let contract_address = receipt.contract_address.unwrap();
    let code = provider.get_code_at(contract_address).await.unwrap();
    assert_eq!(code, ContractTestContext::get_code());
    let filter = Filter::new().address(contract_address);

    // subscribe to events
    let event_sub = ws_provider.subscribe_logs(&filter).await.unwrap();

    // set number - use new .seismic() API
    let tx_input_set_number = ContractTestContext::get_set_number_input_plaintext();
    let tx_set_number: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(tx_input_set_number)
        .with_kind(TxKind::Call(contract_address))
        .into()
        .seismic();

    let pending_tx_set_number = provider.send_transaction(tx_set_number.into()).await.unwrap();
    let receipt_set_number = pending_tx_set_number.get_receipt().await.unwrap();

    assert!(receipt_set_number.status());

    // increment number - use new .seismic() API
    let tx_input_increment_number = ContractTestContext::get_increment_input_plaintext();
    let tx_increment_number: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(tx_input_increment_number)
        .with_kind(TxKind::Call(contract_address))
        .into()
        .seismic();

    let pending_tx_increment_number =
        provider.send_transaction(tx_increment_number.into()).await.unwrap();
    let receipt_increment_number = pending_tx_increment_number.get_receipt().await.unwrap();

    assert!(receipt_increment_number.status());

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
                            num_set_events_received += 1;
                            total_events_received += 1;
                        }
                        else if log.topic0() == Some(&ISeismicCounter::incrementEmit::SIGNATURE_HASH) {
                            num_increment_events_received += 1;
                            total_events_received += 1;
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
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

// ========================================================================
// SeismicCallExt tests — contract.method().seismic().call()/send()
// ========================================================================

sol! {
    #[sol(rpc)]
    contract SeismicCounter {
        function setNumber(suint256 newNumber) public;
        function increment() public;
        function isOdd() public view returns (bool);
    }
}

/// Helper: deploy the SeismicCounter test contract
async fn deploy_test_contract(
    anvil: &AnvilInstance,
) -> (SeismicSignedProvider<SeismicFoundry>, alloy_primitives::Address) {
    let wallet = get_wallet(anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    let plaintext = ContractTestContext::get_deploy_input_plaintext();
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(plaintext).with_kind(TxKind::Create).into();

    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    let contract_address = receipt.contract_address.unwrap();

    (provider, contract_address)
}

#[tokio::test]
async fn test_call_ext_shielded_read() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let contract = SeismicCounter::new(addr, &provider);

    // Shielded read: number is 0, isOdd returns false
    let is_odd = contract.isOdd().seismic().call().await;
    assert!(is_odd.is_ok(), "seismic().call() failed: {:?}", is_odd.unwrap_err());
    assert!(!is_odd.unwrap());
}

#[tokio::test]
async fn test_call_ext_shielded_write_then_read() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let contract = SeismicCounter::new(addr, &provider);

    // Shielded write: setNumber(5)
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(5)))
        .seismic()
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Shielded read: 5 is odd
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "5 should be odd");
}

#[tokio::test]
async fn test_call_ext_shielded_increment() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let contract = SeismicCounter::new(addr, &provider);

    // Shielded write: increment from 0 to 1
    let receipt = contract.increment().seismic().send().await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status());

    // Shielded read: 1 is odd
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "1 should be odd");
}

#[tokio::test]
async fn test_call_ext_shielded_read_write_read() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let contract = SeismicCounter::new(addr, &provider);

    // Shielded read: number is 0, isOdd returns false
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(!is_odd, "0 should not be odd");

    // Shielded write: setNumber(7)
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(7)))
        .seismic()
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Shielded read: 7 is odd
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "7 should be odd");
}

#[tokio::test]
async fn test_call_ext_transparent_call() {
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let contract = SeismicCounter::new(addr, &provider);

    // Transparent read (no .seismic()): standard eth_call
    let is_odd = contract.isOdd().call().await;
    assert!(is_odd.is_ok(), "transparent call() failed: {:?}", is_odd.unwrap_err());
    assert!(!is_odd.unwrap());
}

#[tokio::test]
async fn test_call_ext_transparent_send_then_shielded_read() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let contract = SeismicCounter::new(addr, &provider);

    // Transparent write (no .seismic()): standard send_transaction
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(3)))
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Shielded read to verify the transparent write worked
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "3 should be odd");
}

#[tokio::test]
async fn test_call_ext_security_params_expires_at() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    let current_block = provider.get_block_number().await.unwrap();

    // Shielded read with custom expires_at (generous window) — should succeed
    let is_odd = contract.isOdd().seismic().expires_at(current_block + 50).call().await;
    assert!(is_odd.is_ok(), "seismic().expires_at().call() failed: {:?}", is_odd.unwrap_err());
    assert!(!is_odd.unwrap());
}

#[tokio::test]
async fn test_call_ext_security_params_encryption_nonce() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    // Shielded read with custom encryption nonce — verifies the nonce override
    // doesn't break the encryption/decryption round-trip
    let custom_nonce = alloy_primitives::aliases::U96::from(12345u64);
    let is_odd = contract.isOdd().seismic().encryption_nonce(custom_nonce).call().await;
    assert!(
        is_odd.is_ok(),
        "seismic().encryption_nonce().call() failed: {:?}",
        is_odd.unwrap_err()
    );
    assert!(!is_odd.unwrap());
}

#[tokio::test]
async fn test_call_ext_security_params_send() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    let current_block = provider.get_block_number().await.unwrap();

    // Shielded send with custom expires_at
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(9)))
        .seismic()
        .expires_at(current_block + 50)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Verify the write worked via shielded read
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "9 should be odd");
}

// ========================================================================
// EIP-712 tests — contract.method().seismic().eip712().call()/send()
// ========================================================================

// TODO: Fix circular anvil dependency in integration tests to avoid test failures.
#[ignore]
#[tokio::test]
async fn test_call_ext_eip712_read() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    // EIP-712 signed read: number is 0, isOdd returns false
    let is_odd = contract.isOdd().seismic().eip712().call().await;
    assert!(is_odd.is_ok(), "seismic().eip712().call() failed: {:?}", is_odd.unwrap_err());
    assert!(!is_odd.unwrap());
}

#[ignore]
#[tokio::test]
async fn test_call_ext_eip712_send() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    // EIP-712 signed write: setNumber(11)
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(11)))
        .seismic()
        .eip712()
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Verify via standard shielded read: 11 is odd
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "11 should be odd");
}

#[ignore]
#[tokio::test]
async fn test_call_ext_eip712_write_then_eip712_read() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    // EIP-712 write
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(13)))
        .seismic()
        .eip712()
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // EIP-712 read
    let is_odd = contract.isOdd().seismic().eip712().call().await.unwrap();
    assert!(is_odd, "13 should be odd");
}

// ========================================================================
// seismic_call_with / seismic_send_with tests (provider-level _with variants)
// ========================================================================

#[tokio::test]
async fn test_seismic_call_with_expires_at() {
    use crate::SignedProviderExt;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let current_block = provider.get_block_number().await.unwrap();

    // seismic_call_with using SecurityParams with a custom expires_at
    let is_odd = provider
        .seismic_call_with(
            addr,
            SeismicCounter::isOddCall {},
            crate::SecurityParams::default().expires_at(current_block + 10),
        )
        .await;
    assert!(is_odd.is_ok(), "seismic_call_with failed: {:?}", is_odd.unwrap_err());
    assert!(!is_odd.unwrap());
}

#[tokio::test]
async fn test_seismic_send_with_expires_at() {
    use crate::SignedProviderExt;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;

    let current_block = provider.get_block_number().await.unwrap();

    // seismic_send_with using SecurityParams with a custom expires_at
    let receipt = provider
        .seismic_send_with(
            addr,
            SeismicCounter::setNumberCall {
                newNumber: alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(42)),
            },
            crate::SecurityParams::default().expires_at(current_block + 50),
        )
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Verify the write via a shielded read — 42 is even
    use crate::{SeismicCallExt, ShieldedCallExt};
    let contract = SeismicCounter::new(addr, &provider);
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(!is_odd, "42 should not be odd");
}

// ========================================================================
// with_params on the builder path
// ========================================================================

#[tokio::test]
async fn test_call_ext_with_params_call() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    let current_block = provider.get_block_number().await.unwrap();

    // Use with_params on the builder path for a read
    let is_odd = contract
        .isOdd()
        .seismic()
        .with_params(crate::SecurityParams::default().expires_at(current_block + 10))
        .call()
        .await;
    assert!(is_odd.is_ok(), "with_params().call() failed: {:?}", is_odd.unwrap_err());
    assert!(!is_odd.unwrap());
}

#[tokio::test]
async fn test_call_ext_with_params_send() {
    use crate::{SeismicCallExt, ShieldedCallExt};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let (provider, addr) = deploy_test_contract(&anvil).await;
    let contract = SeismicCounter::new(addr, &provider);

    let current_block = provider.get_block_number().await.unwrap();

    // Use with_params on the builder path for a write
    let receipt = contract
        .setNumber(alloy_primitives::aliases::SUInt(alloy_primitives::U256::from(17)))
        .seismic()
        .with_params(crate::SecurityParams::default().expires_at(current_block + 50))
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert!(receipt.status());

    // Verify: 17 is odd
    let is_odd = contract.isOdd().seismic().call().await.unwrap();
    assert!(is_odd, "17 should be odd");
}

fn get_wallet(anvil: &AnvilInstance) -> SeismicWallet<SeismicFoundry> {
    let bob: PrivateKeySigner = anvil.keys()[1].clone().into();
    SeismicWallet::from(bob)
}
