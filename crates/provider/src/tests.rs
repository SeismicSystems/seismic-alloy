//! Integration tests for SeismicSignedProvider and SeismicUnsignedProvider.
//!
//! All tests spawn a local sanvil (seismic-anvil) instance via `Anvil::at("sanvil").spawn()`.
//! Requires `sanvil` on PATH (can install via `sfoundryup`).
//!
//! ## Test sections
//!
//! 1. **Provider basics** — TEE pubkey, send tx, deploy contracts, seismic_call (signed & unsigned)
//! 2. **Seismic transactions** — send encrypted seismic tx, verify receipt type
//! 3. **Events** — WebSocket subscription for contract events
//! 4. **SeismicCallExt** — contract.method().seismic().call()/send(), security params
//! 5. **EIP-712** — contract.method().seismic().eip712().call()/send()
//! 6. **Provider-level _with variants** — seismic_call_with / seismic_send_with
//! 7. **with_params on builder path** — contract.method().seismic().with_params().call()/send()
//! 8. **Precompile E2E (via contract)** — EncryptedLogs contract exercising RNG + AES-GCM,
//!    cross-check with local decryption
//! 9. **Precompile regression** — RNG entropy varies per transaction
//! 10. **Calldata privacy** — seismic tx input encrypted in getTransaction, legacy tx visible
//! 11. **Gas estimation** — estimate_gas for contract calls
//! 12. **Direct precompile calls** — call each Mercury precompile (0x64-0x69) directly via
//!     eth_call: RNG, ECDH, HKDF, AES-GCM encrypt/decrypt, secp256k1 sign
//! 13. **Private storage enforcement** — SLOAD on private slot blocked, CLOAD via seismic_call
//!     works
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

// ========================================================================
// Provider basics — TEE pubkey, send tx, deploy, seismic_call (signed & unsigned)
// ========================================================================

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

// ========================================================================
// Seismic transactions — send encrypted seismic tx, verify receipt type
// ========================================================================

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

// ========================================================================
// Events — WebSocket subscription for contract events
// ========================================================================

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

// ========================================================================
// Precompile E2E (via contract) — EncryptedLogs exercising RNG + AES-GCM
// ========================================================================

/// End-to-end test of AES precompiles via the EncryptedLogs contract:
/// 1. Deploy contract
/// 2. Set AES key
/// 3. Encrypt "hello world" (uses RNG + AES-encrypt precompiles)
/// 4. Decrypt on-chain via seismic_call (uses AES-decrypt precompile)
/// 5. Cross-check with local AES decryption
#[tokio::test]
async fn test_precompile_aes_encrypt_decrypt() {
    use crate::test_utils::{Encryption, PrecompileTestContext};
    use alloy_dyn_abi::EventExt;
    use alloy_json_abi::{Event, EventParam};
    use alloy_primitives::{
        aliases::{B96, U96},
        IntoLogData, B256,
    };
    use alloy_sol_types::{SolCall, SolValue};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // 1. Deploy EncryptedLogs contract
    let deploy_bytecode = PrecompileTestContext::get_deploy_bytecode();
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(deploy_bytecode).with_kind(TxKind::Create).into();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    let contract_addr = receipt.contract_address.unwrap();

    // 2. Set AES key
    let private_key =
        B256::from(hex!("7e34abdcd62eade2e803e0a8123a0015ce542b380537eff288d6da420bcc2d3b"));
    let set_key_input = Bytes::from(
        Encryption::setAESKeyCall {
            key: alloy_primitives::aliases::SUInt::<256, 4>(alloy_primitives::U256::from_be_bytes(
                *private_key,
            )),
        }
        .abi_encode(),
    );
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(set_key_input)
        .with_kind(TxKind::Call(contract_addr))
        .into();
    provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();

    // 3. Submit "hello world" — triggers RNG precompile for nonce + AES-encrypt precompile
    let message = Bytes::from("hello world");
    type PlaintextType = Bytes;
    let submit_input = Bytes::from(
        Encryption::submitMessageCall { message: message.to_vec().into() }.abi_encode(),
    );
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(submit_input)
        .with_kind(TxKind::Call(contract_addr))
        .into();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();

    // 4. Extract EncryptedMessage event: EncryptedMessage(uint96 indexed nonce, bytes ciphertext)
    let logs = receipt.inner.inner.logs();
    assert_eq!(logs.len(), 1, "Expected exactly one EncryptedMessage event");

    let log_data = logs[0].inner.data.clone();
    let event = Event {
        name: "EncryptedMessage".into(),
        inputs: vec![
            EventParam { ty: "uint96".into(), indexed: true, ..Default::default() },
            EventParam { ty: "bytes".into(), indexed: false, ..Default::default() },
        ],
        anonymous: false,
    };
    let decoded = event.decode_log(&log_data.into_log_data()).unwrap();

    let nonce: U96 =
        U96::from_be_bytes(B96::from_slice(&decoded.indexed[0].abi_encode_packed()).into());
    let ciphertext = Bytes::from(decoded.body[0].abi_encode_packed());

    // 5. On-chain decrypt via seismic_call (uses AES-decrypt precompile)
    let call = Encryption::decryptCall { nonce, ciphertext: ciphertext.clone() };
    let decrypt_input = Bytes::from(call.abi_encode());
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(decrypt_input)
        .with_kind(TxKind::Call(contract_addr))
        .into()
        .seismic();
    let output = provider.seismic_call_raw(SendableTx::Builder(tx.into())).await.unwrap();

    // 6. Cross-check: local AES decryption
    let secp_private =
        seismic_enclave::secp256k1::SecretKey::from_slice(private_key.as_ref()).unwrap();
    let aes_key: [u8; 32] = secp_private.secret_bytes()[0..32].try_into().unwrap();
    let nonce_bytes: [u8; 12] = decoded.indexed[0].abi_encode_packed().try_into().unwrap();
    let decrypted_locally = seismic_enclave::aes_decrypt(&aes_key.into(), &ciphertext, nonce_bytes)
        .expect("Local AES decryption failed");
    assert_eq!(decrypted_locally, message, "Local decryption should match original message");

    // 7. Verify on-chain result matches
    let result_bytes = PlaintextType::abi_decode(&Bytes::from(output))
        .expect("Failed to decode on-chain decrypt output");
    let final_string =
        String::from_utf8(result_bytes.to_vec()).expect("Invalid UTF-8 in decrypted bytes");
    assert_eq!(final_string, "hello world");
}

// ========================================================================
// Precompile regression — RNG entropy varies per transaction
// ========================================================================

/// Regression test: RNG precompile should produce different output per transaction.
/// Before a fix, tx_hash defaulted to B256::ZERO causing identical RNG seeds.
#[tokio::test]
async fn test_precompile_rng_different_per_tx() {
    use crate::test_utils::PrecompileTestContext;
    use alloy_primitives::U256;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Deploy two instances of the RNG caller contract
    let deploy_code = PrecompileTestContext::get_rng_caller_deploy_bytecode();
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(deploy_code.clone())
        .with_kind(TxKind::Create)
        .into();
    let contract_1 = provider
        .send_transaction(tx.into())
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap()
        .contract_address
        .unwrap();

    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(deploy_code).with_kind(TxKind::Create).into();
    let contract_2 = provider
        .send_transaction(tx.into())
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap()
        .contract_address
        .unwrap();

    // Call each contract (triggers RNG precompile, stores result in slot 0)
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(Bytes::new())
        .with_kind(TxKind::Call(contract_1))
        .into();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status(), "Call to contract 1 should succeed");

    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(Bytes::new())
        .with_kind(TxKind::Call(contract_2))
        .into();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status(), "Call to contract 2 should succeed");

    // Read stored RNG values from slot 0
    let rng_a = provider.get_storage_at(contract_1, U256::from(0)).await.unwrap();
    let rng_b = provider.get_storage_at(contract_2, U256::from(0)).await.unwrap();

    assert_ne!(rng_a, U256::ZERO, "RNG output A should not be zero");
    assert_ne!(rng_b, U256::ZERO, "RNG output B should not be zero");
    assert_ne!(
        rng_a, rng_b,
        "RNG precompile should produce different output for different transactions"
    );
}

// ========================================================================
// Calldata privacy — seismic tx input encrypted, legacy tx visible
// ========================================================================

/// Seismic transaction input should be encrypted when retrieved via getTransaction.
/// The plaintext calldata should NOT appear in the stored transaction.
#[tokio::test]
async fn test_seismic_tx_input_is_encrypted() {
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Send a seismic transaction with known plaintext calldata to address(0)
    let plaintext_data = Bytes::from_static(&hex!("deadbeef"));
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(plaintext_data.clone())
        .with_kind(TxKind::Call(Address::ZERO))
        .with_value(alloy_primitives::U256::from(1))
        .into()
        .seismic();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    let tx_hash = receipt.transaction_hash;

    // Retrieve the transaction and check its input field
    let tx: serde_json::Value =
        provider.raw_request("eth_getTransactionByHash".into(), (tx_hash,)).await.unwrap();
    let input = tx["input"].as_str().unwrap_or("");

    // The plaintext "deadbeef" should NOT appear in the stored input
    assert!(
        !input.contains("deadbeef"),
        "Seismic tx input should be encrypted, not contain plaintext. Got: {}",
        input
    );
}

/// Legacy (non-seismic) transaction input should be visible as plaintext.
#[tokio::test]
async fn test_legacy_tx_input_is_plaintext() {
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Send a NON-seismic transaction with known calldata
    let plaintext_data = Bytes::from_static(&hex!("1234567890abcdef"));
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(plaintext_data)
        .with_kind(TxKind::Call(Address::ZERO))
        .with_value(alloy_primitives::U256::from(1))
        .into();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    let tx_hash = receipt.transaction_hash;

    // Retrieve the transaction and check its input field
    let tx: serde_json::Value =
        provider.raw_request("eth_getTransactionByHash".into(), (tx_hash,)).await.unwrap();
    let input = tx["input"].as_str().unwrap_or("");

    // The plaintext should appear as-is
    assert!(
        input.contains("1234567890abcdef"),
        "Legacy tx input SHOULD contain plaintext. Got: {}",
        input
    );
}

// ========================================================================
// Gas estimation — estimate_gas for contract calls
// ========================================================================

/// Gas estimation should work for calls to contracts with shielded storage.
#[tokio::test]
async fn test_gas_estimation() {
    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Deploy contract
    let deploy_input = ContractTestContext::get_deploy_input_plaintext();
    let tx: SeismicTransactionRequest =
        seismic_foundry_tx_builder().with_input(deploy_input).with_kind(TxKind::Create).into();
    let receipt = provider.send_transaction(tx.into()).await.unwrap().get_receipt().await.unwrap();
    let contract_addr = receipt.contract_address.unwrap();

    // Estimate gas for a regular call to the contract
    let call_input = ContractTestContext::get_set_number_input_plaintext();
    let tx: SeismicTransactionRequest = seismic_foundry_tx_builder()
        .with_input(call_input)
        .with_kind(TxKind::Call(contract_addr))
        .into();

    let gas = provider.estimate_gas(tx.into()).await.unwrap();
    assert!(gas > 0, "Gas estimate should be non-zero");
    assert!(gas < 1_000_000, "Gas estimate should be reasonable (< 1M)");
}

// ========================================================================
// Direct precompile calls — call Mercury precompiles (0x64-0x69) via eth_call
// ========================================================================

/// ECDH precompile: derives shared secret from secret key + public key.
#[tokio::test]
async fn test_precompile_ecdh() {
    use crate::precompiles;
    use alloy_primitives::FixedBytes;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    // Use test keys
    let sk = hex!("7e38022030c40773cc561c1cc9c0053e48b0be2cee33c13495f096942ea176ef");
    let pk_secret =
        seismic_enclave::secp256k1::SecretKey::from_slice(&sk).expect("valid secret key");
    let pk_public = pk_secret.public_key(&seismic_enclave::secp256k1::Secp256k1::new());
    let pk_bytes = pk_public.serialize(); // 33 bytes compressed

    let sk_fixed = FixedBytes::<32>::from(sk);
    let result = precompiles::call::ecdh::<SeismicFoundry, _>(&provider, &sk_fixed, &pk_bytes)
        .await
        .unwrap();
    assert_ne!(result, FixedBytes::<32>::ZERO, "ECDH result should not be all zeros");

    // Cross-check: compute ECDH + HKDF locally and verify it matches
    let shared_secret = seismic_enclave::secp256k1::ecdh::SharedSecret::new(&pk_public, &pk_secret);
    let local_aes_key =
        seismic_enclave::derive_aes_key(&shared_secret).expect("HKDF derivation failed");
    assert_eq!(
        result.as_slice(),
        local_aes_key.as_slice(),
        "ECDH precompile should match local derivation"
    );
}

/// HKDF precompile: derives key from input key material.
#[tokio::test]
async fn test_precompile_hkdf_string() {
    use crate::precompiles;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    let input = Bytes::from("HelloHKDF");
    let result =
        precompiles::call::hkdf::<SeismicFoundry, _>(&provider, input.as_ref()).await.unwrap();

    // Known test vector from TS tests
    let expected = hex!("7f527a655fecfa58cd49e00b13684f2df335a3e1a3b9bee749f4d494087038f2");
    assert_eq!(result.as_slice(), expected, "HKDF output should match known test vector");
}

/// HKDF precompile with hex input.
#[tokio::test]
async fn test_precompile_hkdf_hex() {
    use crate::precompiles;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    let input = hex!("1234abcd");
    let result = precompiles::call::hkdf::<SeismicFoundry, _>(&provider, &input).await.unwrap();

    let expected = hex!("67b4c8f882a3a82e4eb12b97aa70652afd62167d0ffd28f81b22e1684c1e8fb2");
    assert_eq!(result.as_slice(), expected, "HKDF hex output should match known test vector");
}

/// secp256k1 precompile: signs a message hash with a secret key.
#[tokio::test]
async fn test_precompile_secp256k1_sign() {
    use crate::precompiles;
    use alloy_primitives::{keccak256, FixedBytes};

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    // Use a known secret key
    let sk_bytes = hex!("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");
    let msg_hash = keccak256("test message for secp256k1 precompile");

    let sk_fixed = FixedBytes::<32>::from(sk_bytes);
    let msg_fixed = FixedBytes::<32>::from(*msg_hash);
    let sig =
        precompiles::call::secp256k1_sign::<SeismicFoundry, _>(&provider, &sk_fixed, &msg_fixed)
            .await
            .unwrap();

    // Verify: the signature should be non-zero
    assert_ne!(
        sig.signature,
        FixedBytes::<64>::ZERO,
        "secp256k1 signature should not be all zeros"
    );

    // Verify the recovery id is valid (0 or 1)
    assert!(sig.recovery_id <= 1, "Recovery id should be 0 or 1, got {}", sig.recovery_id);

    // Cross-check: recover the signer's public key using ecrecover.
    // The precompile signs the raw 32-byte digest (no extra hashing).
    let sk =
        seismic_enclave::secp256k1::SecretKey::from_slice(&sk_bytes).expect("valid secret key");
    let expected_pk = sk.public_key(&seismic_enclave::secp256k1::Secp256k1::new()).serialize();

    let recoverable_sig = seismic_enclave::secp256k1::ecdsa::RecoverableSignature::from_compact(
        sig.signature.as_slice(),
        seismic_enclave::secp256k1::ecdsa::RecoveryId::try_from(sig.recovery_id as i32).unwrap(),
    )
    .expect("valid recoverable signature");
    let msg = seismic_enclave::secp256k1::Message::from_digest(*msg_hash);
    let recovered = seismic_enclave::secp256k1::Secp256k1::new()
        .recover_ecdsa(&msg, &recoverable_sig)
        .expect("recovery should succeed");
    assert_eq!(recovered.serialize(), expected_pk, "Recovered public key should match signer");
}

/// RNG precompile: direct call returns random bytes.
#[tokio::test]
async fn test_precompile_rng_direct() {
    use crate::precompiles;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    let result = precompiles::call::rng::<SeismicFoundry, _>(&provider, 32, &[]).await.unwrap();
    assert_eq!(result.len(), 32, "RNG should return 32 bytes");
    assert_ne!(result, Bytes::from(vec![0u8; 32]), "RNG output should not be all zeros");
}

/// RNG precompile with personalization data.
#[tokio::test]
async fn test_precompile_rng_with_personalization() {
    use crate::precompiles;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    let result = precompiles::call::rng::<SeismicFoundry, _>(&provider, 32, b"test").await.unwrap();
    assert_eq!(result.len(), 32, "RNG with pers should return 32 bytes");
    assert_ne!(result, Bytes::from(vec![0u8; 32]), "RNG with pers should not be all zeros");
}

/// AES-GCM encrypt then decrypt roundtrip via precompiles.
#[tokio::test]
async fn test_precompile_aes_gcm_roundtrip() {
    use crate::precompiles;
    use alloy_primitives::FixedBytes;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let provider =
        crate::SeismicProviderBuilder::new().foundry().connect_http(anvil.endpoint_url());

    let key = FixedBytes::<32>::ZERO;
    let nonce = FixedBytes::<12>::ZERO;
    let plaintext = b"HelloAESGCM";

    let ciphertext =
        precompiles::call::aes_encrypt::<SeismicFoundry, _>(&provider, &key, &nonce, plaintext)
            .await
            .unwrap();
    assert!(!ciphertext.is_empty(), "Ciphertext should not be empty");
    assert_ne!(ciphertext.as_ref(), plaintext, "Ciphertext should differ from plaintext");

    let decrypted =
        precompiles::call::aes_decrypt::<SeismicFoundry, _>(&provider, &key, &nonce, &ciphertext)
            .await
            .unwrap();
    assert_eq!(decrypted.as_ref(), plaintext, "Decrypted should match original plaintext");
}

// ========================================================================
// Private storage enforcement — SLOAD blocked, CLOAD via seismic_call works
// ========================================================================

/// Deploy a contract with both public and private storage, then verify
/// that SLOAD on a private slot is rejected while CLOAD works.
#[tokio::test]
async fn test_private_storage_enforcement() {
    use crate::{test_utils::FlaggedStorageTest, SeismicCallExt, ShieldedCallExt};
    use alloy_primitives::U256;

    let anvil = Anvil::at(SANVIL_PATH).spawn();
    let wallet = get_wallet(&anvil);
    let provider = crate::SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await
        .unwrap();

    // Deploy FlaggedStorageTest contract
    let contract = FlaggedStorageTest::deploy(&provider).await.unwrap();

    // Set public storage: setPublic(42)
    contract.setPublic(U256::from(42)).send().await.unwrap().get_receipt().await.unwrap();

    // Set private storage: setPrivate(99) — suint256 param, auto-encrypts
    contract
        .setPrivate(alloy_primitives::aliases::SUInt(U256::from(99)))
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    // readPublicSload() — should succeed and return 42
    let value = contract.readPublicSload().call().await.unwrap();
    assert_eq!(value, U256::from(42), "readPublicSload should return 42");

    // readPrivateSloadRaw() — raw SLOAD on private slot should be blocked.
    let result = contract.readPrivateSloadRaw().call().await;
    match result {
        Err(_) => {} // error is expected behavior
        Ok(data) => {
            assert_eq!(
                data,
                U256::ZERO,
                "Raw SLOAD on private storage should return 0, not the actual value (99)"
            );
        }
    }

    // readPrivateCload() — CLOAD via seismic signed read, should SUCCEED
    let value = contract.readPrivateCload().seismic().call().await.unwrap();
    assert_eq!(value, U256::from(99), "readPrivateCload should return 99");
}
