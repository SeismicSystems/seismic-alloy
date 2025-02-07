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
- `main` or `master`: this branch only consists of commits from the upstream repository. However it will rarely be up-to-date with upstream. The latest commit from this branch reflects how recently Seismic has merged in upstream commits to the seismic branch
- `seismic`: the default and production branch for these repositories. This includes all Seismic-specific code essential to make our network run
