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
- Adding support for `TxSeismic` in `Provider`
  - Support for encrypting TxSeismic transaction calldata. When a TxSeismic transaction is created, we:
    1. Generate an ephemeral key pair
    2. Use the ephemeral private key and network's public key to generate a shared secret via ECDH
    3. Use the shared secret to encrypt the transaction's calldata
    4. Include the ephemeral public key in the transaction so the network can decrypt the calldata
  - Support for decrypting `eth_call` output. When a signed `eth_call` is made, the network encrypts the output using the ephemeral public key provided in the request. The client can then decrypt this output using the ephemeral private key it generated

## Structure

Seismic's forks of the [reth](https://github.com/paradigmxyz/reth) stack all have the same branch structure:

- `main` or `master`: this branch only consists of commits from the upstream repository. However it will rarely be up-to-date with upstream. The latest commit from this branch reflects how recently Seismic has merged in upstream commits to the seismic branch
- `seismic`: the default and production branch for these repositories. This includes all Seismic-specific code essential to make our network run

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
  - [`alloy-rpc-types-anvil`] - Types for the [seismic-anvil] development node's Ethereum JSON-RPC namespace
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

[`alloy`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/alloy
[`alloy-core`]: https://docs.rs/alloy-core
[`alloy-consensus`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/consensus
[`alloy-consensus-any`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/consensus-any
[`alloy-contract`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/contract
[`alloy-eips`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/eips
[`alloy-genesis`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/genesis
[`alloy-json-rpc`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/json-rpc
[`alloy-network`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/network
[`alloy-network-primitives`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/network-primitives
[`alloy-node-bindings`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/node-bindings
[`alloy-provider`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/provider
[`alloy-pubsub`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/pubsub
[`alloy-rpc-client`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-client
[`alloy-rpc-types`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types
[`alloy-rpc-types-admin`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-admin
[`alloy-rpc-types-anvil`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-anvil
[`alloy-rpc-types-any`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-any
[`alloy-rpc-types-beacon`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-beacon
[`alloy-rpc-types-debug`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-debug
[`alloy-rpc-types-engine`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-engine
[`alloy-rpc-types-eth`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-eth
[`alloy-rpc-types-mev`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-mev
[`alloy-rpc-types-trace`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-trace
[`alloy-rpc-types-txpool`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/rpc-types-txpool
[`alloy-serde`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/serde
[`alloy-signer`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/signer
[`alloy-signer-aws`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/signer-aws
[`alloy-signer-gcp`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/signer-gcp
[`alloy-signer-ledger`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/signer-ledger
[`alloy-signer-local`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/signer-local
[`alloy-signer-trezor`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/signer-trezor
[`alloy-transport`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/transport
[`alloy-transport-http`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/transport-http
[`alloy-transport-ipc`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/transport-ipc
[`alloy-transport-ws`]: https://github.com/SeismicSystems/seismic-alloy/tree/seismic/crates/transport-ws
[publish-subscribe]: https://en.wikipedia.org/wiki/Publish%E2%80%93subscribe_pattern
[AWS KMS]: https://aws.amazon.com/kms
[GCP KMS]: https://cloud.google.com/kms
[Ledger]: https://www.ledger.com
[Trezor]: https://trezor.io
[Serde]: https://serde.rs
[beacon-apis]: https://ethereum.github.io/beacon-APIs
[seismic-anvil]: https://github.com/SeismicSystems/seismic-foundry

## Credits

None of these crates would have been possible without the great work done in:

- [`ethers.js`](https://github.com/ethers-io/ethers.js/)
- [`rust-web3`](https://github.com/tomusdrw/rust-web3/)
- [`ruint`](https://github.com/recmo/uint)
- [`ethabi`](https://github.com/rust-ethereum/ethabi)
- [`ethcontract-rs`](https://github.com/gnosis/ethcontract-rs/)
- [`guac_rs`](https://github.com/althea-net/guac_rs/)
- and of couse [`alloy`](https://github.com/alloy-rs/alloy/)

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
