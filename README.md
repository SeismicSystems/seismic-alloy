# Seismic-Alloy

A Rust toolkit that extends [Alloy](https://github.com/alloy-rs/alloy) to support the Seismic network. Seismic is an EVM-compatible blockchain that allows smart contracts to manage shielded state through encrypted transactions.

## Overview

Seismic-Alloy provides the transaction types, network abstractions, and provider implementations necessary to interact with Seismic. It introduces the Seismic transaction type where calldata is encrypted using ECDH + AEAD before submission, ensuring that only nodes with the network's secret key can decrypt it. Its counterpart is the "signed read" – an eth_call that's sent as a raw transaction, so we can prevent users from spoofing the "from" address. Seismic still supports stock eth_call, but the "from" field is zeroed out

## Features

- Support for all standard Ethereum transaction types (Legacy, EIP-2930, EIP-1559, EIP-4844, EIP-7702)
- Custom Seismic transaction type (type ID `74`) with encryption metadata
- Network abstractions for both testing (Seismic Anvil) and production (Seismic Reth)
- Provider implementations with automatic encryption/decryption
- Genesis configuration with shielded state support

## Prerequisites

- **Rust**: 1.82 or later
- **Seismic Anvil**: Required for running provider tests

### Installing Seismic Anvil via sfoundryup

Seismic Foundry includes `sanvil` (Seismic Anvil), which is required for the test suite.

For installation instructions, see the [Seismic documentation](https://docs.seismic.systems/getting-started/publish-your-docs#install-the-local-development-suite).

## Running Tests

**Before running tests**: Install Seismic Anvil (`sanvil`) to run the full test suite. See installation instructions for [sfoundryup](https://docs.seismic.systems/getting-started/publish-your-docs#install-the-local-development-suite).

Run the full test suite:

```bash
cargo test --workspace
```

Run tests for a specific crate:

```bash
cargo test -p seismic-alloy-consensus
cargo test -p seismic-alloy-network
cargo test -p seismic-alloy-provider
```

The provider test suite spawns local blockchain instances using `sanvil` to test encrypted transactions, provider functionality, and transaction validation.

## Building

Build the project:

```bash
cargo build                # Debug build
cargo build --release      # Release build
```

Check without building:

```bash
cargo check
```

## Code Quality

Format code:

```bash
cargo fmt --all
```

Check formatting:

```bash
cargo fmt --all --check
```

Check with warnings as errors:

```bash
RUSTFLAGS="-D warnings" cargo check
```

## Project Structure

This is a Cargo workspace with six crates:

```
crates/
├── consensus/      # Transaction types, receipts, validation
├── network/        # Network abstractions (Seismic Anvil & Seismic Reth)
├── provider/       # Blockchain interaction layer
├── rpc-types/      # RPC request/response types, genesis
├── genesis/        # Genesis file definitions
└── prelude/        # Convenience re-exports
```

**Dependency chain**: `consensus` → `network` → `provider`

### Key Files

- `crates/consensus/src/transaction/seismic.rs` - Core Seismic transaction type
- `crates/consensus/src/transaction/envelope.rs` - Transaction envelope with Seismic support
- `crates/network/src/seismic_network.rs` - Network trait defining encryption/signing behavior
- `crates/provider/src/provider.rs` - Provider implementations with encryption support
- `crates/rpc-types/src/genesis.rs` - Genesis format with shielded state support

## Resources

- [Alloy Documentation](https://alloy.rs)
- [Seismic Documentation](https://docs.seismic.systems)
- [Repository](https://github.com/SeismicSystems/seismic-alloy)

## License

Licensed under either of the following, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## Contributing

Contributions are welcome! Please ensure:

- All tests pass (`cargo test --workspace`)
- Code is formatted (`cargo fmt --all`)
- No warnings (`RUSTFLAGS="-D warnings" cargo check`)

## Known issues

- Encryption nonce is currently U96 and not B96 – I think really should be changed because decoding fails when there are leading zeroes. Our typescript client has an ugly hack where we will regenerate encryption nonces until there are no leading zeroes, even though this only happens 1/16**2 of the time
- We currently allow estimateGas calls with Seismic transactions to be unsigned (even with from address), even though eth_call requires signatures. This could potentially leak information (curious for thoughts on this – it’s more annoying to change this client side, but obviously doable, and I suspect we’ll need to do this)
- We have not completed our implementation of rotating the root network key, but know this is absolutely critical for preserving forward secrecy among other things. We also have to think about how we store old keys – it might be important to be able to decrypt old Seismic transactions
- We will rely on reth’s unwind command to create checkpoints at the end of each epoch. However, our fork of reth’s unwind command does not correctly handle seismic transactions. We are working on this and will get it in ASAP. Aside from running the node and unwind, there is no other command we plan to use
