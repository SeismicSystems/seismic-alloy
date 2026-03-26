# Seismic-Alloy

A Rust toolkit that extends [Alloy](https://github.com/alloy-rs/alloy) (v1.1.0) to support the [Seismic](https://seismic.systems) network. Requires **Rust 1.82+**.

```toml
[dependencies]
seismic-alloy-provider = { git = "https://github.com/SeismicSystems/seismic-alloy" }
seismic-alloy-network = { git = "https://github.com/SeismicSystems/seismic-alloy" }
seismic-prelude = { git = "https://github.com/SeismicSystems/seismic-alloy" }
```

## Quick Example

```rust
use seismic_prelude::client::*;
use seismic_alloy_network::reth::SeismicReth;

sol! {
    #[sol(rpc)]
    contract SeismicCounter {
        function setNumber(suint256 newNumber) public;
        function isOdd() public view returns (bool);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signer: PrivateKeySigner = "0xYOUR_PRIVATE_KEY".parse()?;
    let wallet = SeismicWallet::<SeismicReth>::from(signer);
    let url = "https://gcp-1.seismictest.net/rpc".parse()?;

    let provider = SeismicProviderBuilder::new()
        .wallet(wallet)
        .connect_http(url)
        .await?;

    let contract = SeismicCounter::new(contract_address, &provider);

    // Shielded write -- setNumber has suint256, auto-encrypts
    contract.setNumber(U256::from(42).into())
        .send()
        .await?
        .get_receipt()
        .await?;

    // Shielded read -- isOdd has no shielded params, use .seismic()
    let is_odd = contract.isOdd().seismic().call().await?;

    // Transparent read -- standard eth_call (from address zeroed)
    let is_odd = contract.isOdd().call().await?;

    Ok(())
}
```

## Overview

Seismic is an EVM-compatible blockchain where smart contracts manage shielded state. Seismic-Alloy provides the transaction types, network abstractions, and provider implementations to interact with it.

Two key primitives:

- **Shielded writes** -- Calldata is encrypted client-side using ECDH + AES-GCM before submission. Only TEE nodes with the network secret key can decrypt it.
- **Signed reads** -- `eth_call` sent as a signed transaction so the node can verify the caller's identity (preventing `from` address spoofing). Seismic still supports standard `eth_call`, but the `from` field is zeroed out.

### Key Management

Each signed provider generates a **secp256k1 keypair** at creation time. This keypair is used for ECDH key agreement with the TEE — deriving the shared AES key that encrypts calldata and decrypts responses. The keypair lives for the lifetime of the provider and is **not automatically rotated**.

For long-running processes, consider periodically recreating the provider to rotate the encryption keypair. The per-transaction encryption nonce ensures that each transaction's ciphertext is unique even with the same keypair, but key rotation is a defense-in-depth measure.

## Features

- **Seismic transaction type (0x4A)** -- Extends standard Ethereum transaction types with encryption metadata
- **Auto-encryption for shielded functions** -- A function is "shielded" if any of its parameters use shielded types (`suint256`, `saddress`, `sbool`, `sbytes`, `sbytesN`). The `sol!` macro detects this and auto-encrypts the entire calldata via `ShieldedCallBuilder` -- `.call()` and `.send()` work directly
- **`.seismic()` call builder** -- `contract.method().seismic().call()` / `.send()` for non-shielded functions that need encryption
- **`SeismicProviderBuilder`** -- Typestate builder for signed (wallet) and unsigned (read-only) providers over HTTP or WebSocket
- **Automatic encryption** -- Filler pipeline handles ECDH key exchange, AES-GCM encryption, and response decryption
- **SecurityParams** -- Per-call `.expires_at()`, `.recent_block_hash()`, `.encryption_nonce()` overrides
- **EIP-712 support** -- `.eip712()` for browser wallet compatibility (MetaMask)
- **Precompile helpers** -- Encode/decode/call wrappers for Seismic's 6 custom precompiles (RNG, ECDH, AES-GCM encrypt/decrypt, HKDF, secp256k1 sign)
- **Full Alloy compatibility** -- All standard `Provider` methods work unchanged

## Project Structure

This is a Cargo workspace with six crates. Each crate is an **additive companion** to its
upstream `alloy-*` counterpart — not a cargo-patch replacement. Both coexist as separate
dependencies in downstream repos (e.g. seismic-reth depends on both `alloy-consensus` and
`seismic-alloy-consensus`).

| Crate       | Upstream counterpart | What it adds                                                                  |
| ----------- | -------------------- | ----------------------------------------------------------------------------- |
| `consensus` | `alloy-consensus`    | `TxSeismic` (type 74), `SeismicTxEnvelope`, `SeismicTypedTransaction`         |
| `genesis`   | `alloy-genesis`      | `Genesis`/`GenesisAccount` with `FlaggedStorage` (private storage at genesis) |
| `network`   | `alloy-network`      | `SeismicNetwork` trait, `SeismicFoundry` (sanvil) and `SeismicReth` impls     |
| `provider`  | `alloy-provider`     | `SeismicSignedProvider`/`SeismicUnsignedProvider` with encryption support     |
| `rpc-types` | `alloy-rpc-types`    | Seismic-specific RPC types (receipts, requests, genesis, block simulation)    |
| `examples`  | —                    | Runnable examples (basic_contract, eip712, precompiles, etc.)                 |
| `prelude`   | —                    | Convenience re-exports of the above                                           |

**Dependency chain**: `consensus` → `network` → `provider`

## Prerequisites

- **sanvil**: Required for running provider tests and examples (see below)

## Building

```bash
cargo build                # Debug build
cargo build --release      # Release build
cargo check                # Fast validation without building
```

## Running Tests

The provider tests require `sanvil` (Seismic Anvil) in `$PATH` or `$HOME/.seismic/bin/`. Install via [sfoundryup](https://docs.seismic.systems/getting-started/publish-your-docs#install-the-local-development-suite).

```bash
cargo test --workspace                  # Full test suite
cargo test -p seismic-alloy-provider    # Provider tests only
cargo test -p seismic-alloy-consensus   # Consensus tests only
```

## Code Quality

```bash
cargo +nightly fmt --all                         # Format code
cargo +nightly fmt --all --check                 # Check formatting (CI)
RUSTFLAGS="-D warnings" cargo check --all-targets  # Warnings as errors (CI)
```

## Documentation

Full API documentation is available at [docs.seismic.systems](https://docs.seismic.systems).

## Resources

- [Seismic Documentation](https://docs.seismic.systems)
- [Alloy Documentation](https://alloy.rs)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please ensure:

- All tests pass (`cargo test --workspace`)
- Code is formatted (`cargo +nightly fmt --all`)
- No warnings (`RUSTFLAGS="-D warnings" cargo check --all-targets`)
