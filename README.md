# Seismic-Alloy

A Rust toolkit that extends [Alloy](https://github.com/alloy-rs/alloy) (v1.1.0) to support the [Seismic](https://seismic.systems) network. Requires **Rust 1.82+**.

```toml
[dependencies]
seismic-alloy = { git = "https://github.com/SeismicSystems/seismic-alloy" }
```

## Quick Example

```rust
use seismic_alloy_network::{reth::SeismicReth, wallet::SeismicWallet};
use seismic_alloy_provider::{SeismicCallExt, SeismicProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use alloy_primitives::U256;

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

    // Shielded write -- calldata encrypted, sent as a Seismic transaction
    contract.setNumber(U256::from(42).into())
        .seismic()
        .send()
        .await?
        .get_receipt()
        .await?;

    // Shielded read -- encrypted eth_call with response decryption
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

## Features

- **`SeismicProviderBuilder`** -- Typestate builder for signed (wallet) and unsigned (read-only) providers over HTTP or WebSocket
- **`.seismic()` call builder** -- `contract.method().seismic().call()` / `.send()` for ergonomic shielded operations
- **Automatic encryption** -- Filler pipeline handles ECDH key exchange, AES-GCM encryption, and response decryption
- **SecurityParams** -- Per-call `.expires_at()`, `.recent_block_hash()`, `.encryption_nonce()` overrides
- **EIP-712 support** -- `.seismic().eip712()` for browser wallet compatibility (MetaMask)
- **Precompile helpers** -- Encode/decode/call wrappers for Seismic's 6 custom precompiles (RNG, ECDH, AES-GCM encrypt/decrypt, HKDF, secp256k1 sign)
- **Seismic transaction type (0x4A)** -- Extends standard Ethereum transaction types with encryption metadata
- **Full Alloy compatibility** -- All standard `Provider` methods work unchanged

## Project Structure

```
crates/
├── consensus/      # Seismic transaction types, receipts, validation
├── network/        # SeismicNetwork trait, SeismicReth, SeismicFoundry
├── provider/       # SeismicProviderBuilder, fillers, precompile helpers
├── rpc-types/      # Seismic-specific RPC request/response types
├── genesis/        # Genesis configuration with shielded state support
├── examples/       # Runnable examples (basic_contract, etc.)
└── prelude/        # Internal re-exports for Seismic's Foundry and Reth forks
```

**Dependency chain**: `consensus` → `network` → `provider`

Most applications need only two crates:

```toml
[dependencies]
seismic-alloy-provider = { git = "https://github.com/SeismicSystems/seismic-alloy" }
seismic-alloy-network = { git = "https://github.com/SeismicSystems/seismic-alloy" }
```

## Prerequisites

- **Rust**: 1.82 or later
- **sanvil**: Required for running provider tests (see below)

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
cargo fmt --all                         # Format code
cargo fmt --all --check                 # Check formatting (CI)
RUSTFLAGS="-D warnings" cargo check     # Warnings as errors (CI)
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
- Code is formatted (`cargo fmt --all`)
- No warnings (`RUSTFLAGS="-D warnings" cargo check`)
