# Seismic-Alloy

Rust toolkit extending [Alloy](https://github.com/alloy-rs/alloy) for the Seismic network — an EVM-compatible blockchain with **shielded state**. Adds the Seismic transaction type (type ID `74`, 0x4A) where calldata is encrypted via ECDH + AEAD before submission, decryptable only by nodes holding the network secret key.

## Build

Cargo workspace with 6 crates. Requires Rust 1.82+ (stable) and nightly for formatting.

### macOS (arm64/x86_64)

```bash
# Prerequisites: Rust stable + nightly
rustup toolchain install stable nightly

# Build
cargo build

# Verify
cargo build 2>&1 | tail -1
# Expected: Finished `dev` profile [unoptimized + debuginfo] target(s) in ...
```

### Linux (Ubuntu)

```bash
# Prerequisites
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev
rustup toolchain install stable nightly

# Build
cargo build
```

## Test

Tests require `sanvil` (Seismic Anvil) in `$PATH` or `$HOME/.seismic/bin/`.

### Install sanvil

```bash
# Via sfoundryup (see https://github.com/SeismicSystems/seismic-foundry)
bash <(curl -fsSL https://raw.githubusercontent.com/SeismicSystems/seismic-foundry/seismic/sfoundryup/install)
sfoundryup
```

### Run tests

```bash
cargo test --workspace                        # Full suite (54 tests)
cargo test -p seismic-alloy-consensus         # Consensus crate only (15 tests)
cargo test -p seismic-alloy-provider          # Provider crate only (8 tests, needs sanvil)
cargo test -p seismic-alloy-genesis           # Genesis crate only (28 tests)
cargo test -p seismic-alloy-rpc-types         # RPC types only (6 tests)
```

### CI checks (all 4 must pass)

```bash
cargo +nightly fmt --all --check              # Format check (nightly required)
cargo build                                   # Build
RUSTFLAGS="-D warnings" cargo check           # Warnings as errors
cargo test --workspace                        # Tests
```

### Format code

```bash
cargo +nightly fmt --all                      # Auto-format (nightly required)
```

## Project Layout

```
crates/
├── consensus/      Core Seismic transaction type (TxSeismic, TxSeismicElements)
│                   Envelopes, receipts, tx type enum. Dependency chain root.
├── network/        SeismicNetwork trait + SeismicFoundry (testing) / SeismicReth (production)
├── provider/       SeismicSignedProvider, SeismicUnsignedProvider, SeismicProviderExt
│                   Integration tests live here (require sanvil)
├── rpc-types/      RPC request/response types, genesis config, receipts
├── genesis/        Genesis file definitions with FlaggedStorage (private state)
└── prelude/        Convenience re-exports for Foundry and Reth consumers
```

**Dependency chain**: `consensus` → `network` → `provider` → `prelude`

## Key Files

| File                                           | What                                                              |
| ---------------------------------------------- | ----------------------------------------------------------------- |
| `crates/consensus/src/transaction/seismic.rs`  | `TxSeismic`, `TxSeismicElements`, encryption metadata             |
| `crates/consensus/src/transaction/envelope.rs` | `SeismicTxEnvelope` (all tx types + Seismic)                      |
| `crates/consensus/src/transaction/typed.rs`    | `SeismicTypedTransaction` union type                              |
| `crates/consensus/src/transaction/tx_type.rs`  | `SeismicTxType` enum, `SEISMIC_TX_TYPE_ID = 74`                   |
| `crates/network/src/seismic_network.rs`        | `SeismicNetwork` trait (encryption/signing abstraction)           |
| `crates/network/src/foundry/mod.rs`            | `SeismicFoundry` — test network impl                              |
| `crates/network/src/reth/mod.rs`               | `SeismicReth` — production network impl                           |
| `crates/provider/src/provider.rs`              | Provider impls + all integration tests                            |
| `crates/provider/src/test_utils.rs`            | Test contract helpers (`ISeismicCounter`, bytecode)               |
| `Cargo.toml`                                   | Workspace config, pinned fork dependencies in `[patch.crates-io]` |

## Pinned Dependencies

All custom forks are commit-pinned in `Cargo.toml`'s `[patch.crates-io]`. Do not bump without coordinated testing.

| Fork                                    | Crate(s)                                                                                                               |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `SeismicSystems/enclave.git`            | `seismic-enclave`                                                                                                      |
| `SeismicSystems/seismic-alloy-core.git` | `alloy-primitives`, `alloy-sol-types`, `alloy-json-abi`, `alloy-dyn-abi`, `alloy-sol-macro-*`, `alloy-sol-type-parser` |
| `SeismicSystems/seismic-trie.git`       | `alloy-trie`                                                                                                           |
| `SeismicSystems/seismic-revm.git`       | `revm`, `seismic-revm`                                                                                                 |

Alloy is pinned to **exact version 1.1.0** (`=1.1.0`).

## Code Style

- Nightly rustfmt with config in `rustfmt.toml`
- Workspace lints: `missing-docs = "warn"`, `unused-must-use = "deny"`, `rust-2018-idioms = "deny"`, clippy all warnings
- CI treats all warnings as errors (`RUSTFLAGS="-D warnings"`)

## Gotchas

- **Seismic TX type ID is `74` (0x4A)** — hardcoded in `tx_type.rs`. Do not change without chain upgrade coordination.
- **Encryption pubkeys** are 33-byte compressed secp256k1 (`FixedBytes<33>`). Never use 65-byte uncompressed.
- **Seismic txs use legacy gas params** (`gas_price` + `gas_limit`), not EIP-1559 style.
- **Tests require sanvil** — standard GitHub runners won't work. CI uses a self-hosted runner with `$HOME/.seismic/bin` in PATH.
- **Do not bump Alloy** past 1.1.0 without testing all forks.
- **Do not modify fork repos** (seismic-enclave, seismic-revm, etc.) from this repo.

## Troubleshooting

| Problem                                                              | Fix                                                                                                                                         |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo +nightly fmt` fails with "no nightly toolchain"               | `rustup toolchain install nightly`                                                                                                          |
| Provider tests fail with "AES decryption failed" panic in sanvil     | sanvil version mismatch — reinstall with `sfoundryup` to get a version compatible with current seismic-enclave commit                       |
| Provider tests fail with "connection refused" or "IncompleteMessage" | sanvil crashed during test. Often follows the AES error above. Same fix: update sanvil.                                                     |
| `cargo test` can't find `sanvil`                                     | Ensure `sanvil` is in `$PATH` or `$HOME/.seismic/bin/`. Run: `which sanvil` to verify.                                                      |
| Build fails fetching git dependencies                                | Ensure SSH keys or HTTPS credentials are configured for GitHub. The `[patch.crates-io]` section fetches from multiple SeismicSystems repos. |
| Warnings cause `cargo check` to fail                                 | Expected — CI runs `RUSTFLAGS="-D warnings"`. Fix all warnings before pushing.                                                              |
