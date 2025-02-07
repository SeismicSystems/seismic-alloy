# Seismic Alloy

This repository contains Seismic's fork of Alloy

The upstream repository lives [here](https://github.com/alloy-rs/alloy). This fork is up-to-date with it through commit `de01884`. You can see this by viewing the [main](https://github.com/SeismicSystems/seismic-alloy/tree/main) branch on this repository

You can view all of our changes vs. upstream on this [pull request](https://github.com/SeismicSystems/seismic-alloy/pull/2). The sole purpose of this PR is display our diff; it will never be merged in to the main branch of this repo

## Main changes
- Adding the `TxSeismic` transaction type (with tx_type `0x4a` or `74`). This transaction type introduces fields that Seismic uses to secure its blockchain.
  - `encryption_pubkey`: this field represents the EOA's ephemerally generated public key. This is NOT the public key associated with their Ethereum address. When a Seismic transaction is sent to the chain, the calldata is encrypted using a shared secret. This secret is generated from the network's key and the aforementioned ephemeral key. We pass this into the transaction so the network to decrypt the EOA's calldata (in the `input` field like other transactions)
  - `message_version`: At least temporarily, Seismic allows transactions to be sent to the network in two ways: (1) the "normal" way, via sending raw transaction bytes, and (2), signing an EIP-712 typed message where the data is a `TxSeismic`. We added support for (2) because we unfortunately couldn't figure out how to get browser extension wallets to sign Seismic Transactions. This may be removed in the future. Permitted values of this field:
    - `0`: marks that this transaction was signed via the standard `signTransaction` way, and therefore sent to the network as raw transaction bytes
    - `2`: marks that this transaction was sent as EIP-712 typed data
    - We have reserved the field `1` for when we want to support transactions signed via `personal_sign` (for e.g. hardware wallets)
- Adding two enums, `SeismicCallRequest` and `SeismicRawTxRequest`. These are types to support Seismic's extensions to the Ethereum's RPC methods `eth_call` and `eth_sendRawTransaction` respectively
  - `SeismicCallRequest`. On Seismic, you can `eth_call` in two ways
    - The normal way, by submitting a transaction request. This will behave as normal, except if you set the `from` field, it will be overridden to the zero address. We do this to disallow users from making eth calls from addresses they do not own
    - By making a "signed call". This is the same as a normal transaction request, but is also associated with a signature. In this case, the `from` field will be populated with the signer's address, and is passed on to smart contracts. Therefore smart contracts can be sure that `msg.sender` cannot be spoofed, as they are authenticated. This is the only way to make an eth_call where the `from` field is specified. You can do this in two ways:
      - With a raw transaction payload (e.g. bytes)
      - With EIP-712 signed typed data (for browser wallet support)
  - `SeismicRawTxRequest`. On Seismic, you can send a raw transaction in two ways:
    - The normal way, with raw transaction bytes
    - With EIP-712 typed data, as alluded to in the section discussing `message_version`.

## Structure

Seismic's forks of the [reth](https://github.com/paradigmxyz/reth) stack all have the same branch structure:
- `main` or `master`: this branch consists of commits purely from the upstream repository. However it will rarely be up-to-date with upstream. The latest commit from this branch reflects how recently Seismic has merged in upstream commits to the seismic branch
- `seismic`: the default and production branch for these repositories. This includes all Seismic-specific code essential to make our network run

# Alloy

Alloy connects applications to blockchains.

Alloy is a rewrite of [`ethers-rs`] from the ground up, with exciting new
features, high performance, and excellent [docs](https://docs.rs/alloy).

We also have a [book](https://alloy.rs/) on all things Alloy and many [examples](https://github.com/alloy-rs/examples) to help you get started.

[![Telegram chat][telegram-badge]][telegram-url]

[`ethers-rs`]: https://github.com/gakonst/ethers-rs
[telegram-badge]: https://img.shields.io/endpoint?color=neon&style=for-the-badge&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fethers_rs
[telegram-url]: https://t.me/ethers_rs

## Installation

Alloy consists of a number of crates that provide a range of functionality essential for interfacing with any Ethereum-based blockchain.

The easiest way to get started is to add the `alloy` crate with the `full` feature flag from the command-line using Cargo:

```sh
cargo add alloy --features full
```

Alternatively, you can add the following to your `Cargo.toml` file:

```toml
alloy = { version = "0.3", features = ["full"] }
```

For a more fine-grained control over the features you wish to include, you can add the individual crates to your `Cargo.toml` file, or use the `alloy` crate with the features you need.

A comprehensive list of available features can be found on [docs.rs](https://docs.rs/crate/alloy/latest/features) or in the [`alloy` crate's `Cargo.toml`](https://github.com/alloy-rs/alloy/blob/main/crates/alloy/Cargo.toml).

## Overview

This repository contains the following crates:

- [`alloy`]: Meta-crate for the entire project, including [`alloy-core`]
- [`alloy-consensus`] - Ethereum consensus interface
  - [`alloy-consensus-any`] - Catch-all consensus interface for multiple networks
- [`alloy-contract`] - Interact with on-chain contracts
- [`alloy-eips`] - Ethereum Improvement Proposal (EIP) implementations
- [`alloy-genesis`] - Ethereum genesis file definitions
- [`alloy-json-rpc`] - Core data types for JSON-RPC 2.0 clients
- [`alloy-network`] - Network abstraction for RPC types
  - [`alloy-network-primitives`] - Primitive types for the network abstraction
- [`alloy-node-bindings`] - Ethereum execution-layer client bindings
- [`alloy-provider`] - Interface with an Ethereum blockchain
- [`alloy-pubsub`] - Ethereum JSON-RPC [publish-subscribe] tower service and type definitions
- [`alloy-rpc-client`] - Low-level Ethereum JSON-RPC client implementation
- [`alloy-rpc-types`] - Meta-crate for all Ethereum JSON-RPC types
  - [`alloy-rpc-types-admin`] - Types for the `admin` Ethereum JSON-RPC namespace
  - [`alloy-rpc-types-anvil`] - Types for the [Anvil] development node's Ethereum JSON-RPC namespace
  - [`alloy-rpc-types-any`] - Types for JSON-RPC namespaces across multiple networks
  - [`alloy-rpc-types-beacon`] - Types for the [Ethereum Beacon Node API][beacon-apis]
  - [`alloy-rpc-types-debug`] - Types for the `debug` Ethereum JSON-RPC namespace
  - [`alloy-rpc-types-engine`] - Types for the `engine` Ethereum JSON-RPC namespace
  - [`alloy-rpc-types-eth`] - Types for the `eth` Ethereum JSON-RPC namespace
  - [`alloy-rpc-types-mev`] - Types for the MEV bundle JSON-RPC namespace
  - [`alloy-rpc-types-trace`] - Types for the `trace` Ethereum JSON-RPC namespace
  - [`alloy-rpc-types-txpool`] - Types for the `txpool` Ethereum JSON-RPC namespace
- [`alloy-serde`] - [Serde]-related utilities
- [`alloy-signer`] - Ethereum signer abstraction
  - [`alloy-signer-aws`] - [AWS KMS] signer implementation
  - [`alloy-signer-gcp`] - [GCP KMS] signer implementation
  - [`alloy-signer-ledger`] - [Ledger] signer implementation
  - [`alloy-signer-local`] - Local (private key, keystore, mnemonic, YubiHSM) signer implementations
  - [`alloy-signer-trezor`] - [Trezor] signer implementation
- [`alloy-transport`] - Low-level Ethereum JSON-RPC transport abstraction
  - [`alloy-transport-http`] - HTTP transport implementation
  - [`alloy-transport-ipc`] - IPC transport implementation
  - [`alloy-transport-ws`] - WS transport implementation

[`alloy`]: https://github.com/alloy-rs/alloy/tree/main/crates/alloy
[`alloy-core`]: https://docs.rs/alloy-core
[`alloy-consensus`]: https://github.com/alloy-rs/alloy/tree/main/crates/consensus
[`alloy-consensus-any`]: https://github.com/alloy-rs/alloy/tree/main/crates/consensus-any
[`alloy-contract`]: https://github.com/alloy-rs/alloy/tree/main/crates/contract
[`alloy-eips`]: https://github.com/alloy-rs/alloy/tree/main/crates/eips
[`alloy-genesis`]: https://github.com/alloy-rs/alloy/tree/main/crates/genesis
[`alloy-json-rpc`]: https://github.com/alloy-rs/alloy/tree/main/crates/json-rpc
[`alloy-network`]: https://github.com/alloy-rs/alloy/tree/main/crates/network
[`alloy-network-primitives`]: https://github.com/alloy-rs/alloy/tree/main/crates/network-primitives
[`alloy-node-bindings`]: https://github.com/alloy-rs/alloy/tree/main/crates/node-bindings
[`alloy-provider`]: https://github.com/alloy-rs/alloy/tree/main/crates/provider
[`alloy-pubsub`]: https://github.com/alloy-rs/alloy/tree/main/crates/pubsub
[`alloy-rpc-client`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-client
[`alloy-rpc-types`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types
[`alloy-rpc-types-admin`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-admin
[`alloy-rpc-types-anvil`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-anvil
[`alloy-rpc-types-any`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-any
[`alloy-rpc-types-beacon`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-beacon
[`alloy-rpc-types-debug`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-debug
[`alloy-rpc-types-engine`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-engine
[`alloy-rpc-types-eth`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-eth
[`alloy-rpc-types-mev`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-mev
[`alloy-rpc-types-trace`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-trace
[`alloy-rpc-types-txpool`]: https://github.com/alloy-rs/alloy/tree/main/crates/rpc-types-txpool
[`alloy-serde`]: https://github.com/alloy-rs/alloy/tree/main/crates/serde
[`alloy-signer`]: https://github.com/alloy-rs/alloy/tree/main/crates/signer
[`alloy-signer-aws`]: https://github.com/alloy-rs/alloy/tree/main/crates/signer-aws
[`alloy-signer-gcp`]: https://github.com/alloy-rs/alloy/tree/main/crates/signer-gcp
[`alloy-signer-ledger`]: https://github.com/alloy-rs/alloy/tree/main/crates/signer-ledger
[`alloy-signer-local`]: https://github.com/alloy-rs/alloy/tree/main/crates/signer-local
[`alloy-signer-trezor`]: https://github.com/alloy-rs/alloy/tree/main/crates/signer-trezor
[`alloy-transport`]: https://github.com/alloy-rs/alloy/tree/main/crates/transport
[`alloy-transport-http`]: https://github.com/alloy-rs/alloy/tree/main/crates/transport-http
[`alloy-transport-ipc`]: https://github.com/alloy-rs/alloy/tree/main/crates/transport-ipc
[`alloy-transport-ws`]: https://github.com/alloy-rs/alloy/tree/main/crates/transport-ws

[publish-subscribe]: https://en.wikipedia.org/wiki/Publish%E2%80%93subscribe_pattern
[AWS KMS]: https://aws.amazon.com/kms
[GCP KMS]: https://cloud.google.com/kms
[Ledger]: https://www.ledger.com
[Trezor]: https://trezor.io
[Serde]: https://serde.rs
[beacon-apis]: https://ethereum.github.io/beacon-APIs
[Anvil]: https://github.com/foundry-rs/foundry

## Supported Rust Versions (MSRV)

<!--
When updating this, also update:
- clippy.toml
- Cargo.toml
- .github/workflows/ci.yml
-->

The current MSRV (minimum supported rust version) is 1.81.

Alloy will keep a rolling MSRV policy of **at least** two versions behind the
latest stable release (so if the latest stable release is 1.58, we would
support 1.56).

Note that the MSRV is not increased automatically, and only as part of a patch
(pre-1.0) or minor (post-1.0) release.

## Contributing

Thanks for your help improving the project! We are so happy to have you! We have
[a contributing guide](./CONTRIBUTING.md) to help you get involved in the
Alloy project.

Pull requests will not be merged unless CI passes, so please ensure that your
contribution follows the linting rules and passes clippy.

## Note on `no_std`

Because these crates are primarily network-focused, we do not intend to support
`no_std` for most of them at this time.

The following crates support `no_std`:

- alloy-eips
- alloy-genesis
- alloy-serde
- alloy-consensus

If you would like to add `no_std` support to a crate, please make sure to update
`scripts/check_no_std.sh` as well.

## Credits

None of these crates would have been possible without the great work done in:

- [`ethers.js`](https://github.com/ethers-io/ethers.js/)
- [`rust-web3`](https://github.com/tomusdrw/rust-web3/)
- [`ruint`](https://github.com/recmo/uint)
- [`ethabi`](https://github.com/rust-ethereum/ethabi)
- [`ethcontract-rs`](https://github.com/gnosis/ethcontract-rs/)
- [`guac_rs`](https://github.com/althea-net/guac_rs/)

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
</sub>
