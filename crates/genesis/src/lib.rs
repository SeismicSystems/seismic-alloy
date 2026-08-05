//! Seismic genesis types — extends upstream [`alloy_genesis`] with [`FlaggedStorage`] support.
//!
//! A genesis file in a Seismic blockchain is a JSON-formatted file used to define the initial
//! state of the blockchain at the time of its creation. Unlike standard Ethereum, Seismic
//! allows for private state at genesis.
//!
//! # Relationship to `alloy-genesis`
//!
//! This crate is an **additive companion** to upstream `alloy-genesis`, not a cargo-patch
//! replacement. Both crates coexist as separate dependencies (downstream crates like
//! seismic-reth depend on both `alloy-genesis` and `seismic-alloy-genesis`).
//! This follows the same pattern as the other seismic-alloy crates (`seismic-alloy-consensus`,
//! `seismic-alloy-network`, etc.), which all add Seismic-specific types alongside their
//! upstream counterparts.
//!
//! The key difference is that storage values use [`FlaggedStorage`] (which carries an
//! `is_private` flag) instead of plain `B256`:
//!
//! | | `alloy-genesis` (upstream) | `seismic-alloy-genesis` (this crate) |
//! |---|---|---|
//! | `GenesisAccount.storage` | `Option<BTreeMap<B256, B256>>` | `Option<BTreeMap<B256, FlaggedStorage>>` |
//! | `into_trie_account()` | passes `U256` to `storage_root_unhashed` | passes `FlaggedStorage` directly |
//! | deserialization | hex strings only | hex strings (→ public) **and** `{value, is_private}` objects |
//!
//! This crate reuses types that don't need modification ([`ChainConfig`], [`CliqueConfig`])
//! directly from `alloy-genesis`, and provides [`From`] conversions to go from upstream
//! types to Seismic types (defaulting storage to public):
//! - `From<alloy_genesis::Genesis> for Genesis`
//! - `From<alloy_genesis::GenesisAccount> for GenesisAccount`
//!
//! # Usage across the Seismic codebase
//!
//! - **seismic-reth** depends on both crates. Privacy-aware code uses
//!   `seismic_alloy_genesis::{Genesis, GenesisAccount}`. Upstream reth code that still references
//!   `alloy_genesis::Genesis` converts via `.into()`.
//! - **seismic-foundry** uses upstream `alloy_genesis` directly (anvil/forge don't need
//!   privacy-aware genesis handling).
//!
//! # Why `alloy-genesis` still compiles (and why it matters)
//!
//! Because this crate depends on `alloy-genesis`, and our workspace patches `alloy-trie`
//! to `seismic-trie`, the upstream `alloy_genesis::GenesisAccount::into_trie_account()`
//! ends up calling `seismic-trie::storage_root_unhashed<T: Into<FlaggedStorage>>` with
//! `U256` values. This requires `From<U256> for FlaggedStorage` to exist even though the
//! Seismic codebase never calls that upstream code path. See the `From<U256>` impl on
//! `FlaggedStorage` for details.

#![doc = include_str!(".././README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/alloy-rs/core/main/assets/alloy.jpg",
    html_favicon_url = "https://raw.githubusercontent.com/alloy-rs/core/main/assets/favicon.ico"
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, string::String};
use alloy_primitives::{keccak256, Address, Bytes, FlaggedStorage, B256, U256};
use alloy_serde::storage::from_bytes_to_b256;
use alloy_trie::{TrieAccount, EMPTY_ROOT_HASH, KECCAK_EMPTY};
use core::str::FromStr;
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};

use alloy_genesis::{ChainConfig, CliqueConfig};

impl From<alloy_genesis::Genesis> for Genesis {
    fn from(genesis: alloy_genesis::Genesis) -> Self {
        Self {
            config: genesis.config,
            nonce: genesis.nonce,
            timestamp: genesis.timestamp,
            extra_data: genesis.extra_data,
            gas_limit: genesis.gas_limit,
            difficulty: genesis.difficulty,
            mix_hash: genesis.mix_hash,
            coinbase: genesis.coinbase,
            alloc: genesis
                .alloc
                .into_iter()
                .map(|(addr, account)| (addr, account.into()))
                .collect(),
            base_fee_per_gas: genesis.base_fee_per_gas,
            excess_blob_gas: genesis.excess_blob_gas,
            blob_gas_used: genesis.blob_gas_used,
            number: genesis.number,
        }
    }
}

/// The genesis block specification.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Genesis {
    /// The fork configuration for this network.
    #[serde(default)]
    pub config: ChainConfig,
    /// The genesis header nonce.
    #[serde(with = "alloy_serde::quantity")]
    pub nonce: u64,
    /// The genesis header timestamp.
    #[serde(with = "alloy_serde::quantity")]
    pub timestamp: u64,
    /// The genesis header extra data.
    pub extra_data: Bytes,
    /// The genesis header gas limit.
    #[serde(with = "alloy_serde::quantity")]
    pub gas_limit: u64,
    /// The genesis header difficulty.
    pub difficulty: U256,
    /// The genesis header mix hash.
    pub mix_hash: B256,
    /// The genesis header coinbase address.
    pub coinbase: Address,
    /// The initial state of accounts in the genesis block.
    pub alloc: BTreeMap<Address, GenesisAccount>,
    // NOTE: the following fields:
    // * base_fee_per_gas
    // * excess_blob_gas
    // * blob_gas_used
    // * number
    // should NOT be set in a real genesis file, but are included here for compatibility with
    // consensus tests, which have genesis files with these fields populated.
    /// The genesis header base fee
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub base_fee_per_gas: Option<u128>,
    /// The genesis header excess blob gas
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub excess_blob_gas: Option<u64>,
    /// The genesis header blob gas used
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub blob_gas_used: Option<u64>,
    /// The genesis block number
    #[serde(default, skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt")]
    pub number: Option<u64>,
}

impl Genesis {
    /// Creates a chain config for Clique using the given chain id and funds the given address with
    /// max coins.
    ///
    /// Enables all hard forks up to London at genesis.
    pub fn clique_genesis(chain_id: u64, signer_addr: Address) -> Self {
        // set up a clique config with an instant sealing period and short (8 block) epoch
        let clique_config = CliqueConfig { period: Some(0), epoch: Some(8) };

        let config = ChainConfig {
            chain_id,
            eip155_block: Some(0),
            eip150_block: Some(0),
            eip158_block: Some(0),

            homestead_block: Some(0),
            byzantium_block: Some(0),
            constantinople_block: Some(0),
            petersburg_block: Some(0),
            istanbul_block: Some(0),
            muir_glacier_block: Some(0),
            berlin_block: Some(0),
            london_block: Some(0),
            clique: Some(clique_config),
            ..Default::default()
        };

        // fund account
        let alloc = BTreeMap::from([(
            signer_addr,
            GenesisAccount { balance: U256::MAX, ..Default::default() },
        )]);

        // put signer address in the extra data, padded by the required amount of zeros
        // Clique issue: https://github.com/ethereum/EIPs/issues/225
        // Clique EIP: https://eips.ethereum.org/EIPS/eip-225
        //
        // The first 32 bytes are vanity data, so we will populate it with zeros
        // This is followed by the signer address, which is 20 bytes
        // There are 65 bytes of zeros after the signer address, which is usually populated with the
        // proposer signature. Because the genesis does not have a proposer signature, it will be
        // populated with zeros.
        let extra_data_bytes = [&[0u8; 32][..], signer_addr.as_slice(), &[0u8; 65][..]].concat();
        let extra_data = extra_data_bytes.into();

        Self {
            config,
            alloc,
            difficulty: U256::from(1),
            gas_limit: 5_000_000,
            extra_data,
            ..Default::default()
        }
    }

    /// Set the nonce.
    pub const fn with_nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    /// Set the timestamp.
    pub const fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Set the extra data.
    pub fn with_extra_data(mut self, extra_data: Bytes) -> Self {
        self.extra_data = extra_data;
        self
    }

    /// Set the gas limit.
    pub const fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    /// Set the difficulty.
    pub const fn with_difficulty(mut self, difficulty: U256) -> Self {
        self.difficulty = difficulty;
        self
    }

    /// Set the mix hash of the header.
    pub const fn with_mix_hash(mut self, mix_hash: B256) -> Self {
        self.mix_hash = mix_hash;
        self
    }

    /// Set the coinbase address.
    pub const fn with_coinbase(mut self, address: Address) -> Self {
        self.coinbase = address;
        self
    }

    /// Set the base fee.
    pub const fn with_base_fee(mut self, base_fee: Option<u128>) -> Self {
        self.base_fee_per_gas = base_fee;
        self
    }

    /// Set the excess blob gas.
    pub const fn with_excess_blob_gas(mut self, excess_blob_gas: Option<u64>) -> Self {
        self.excess_blob_gas = excess_blob_gas;
        self
    }

    /// Set the blob gas used.
    pub const fn with_blob_gas_used(mut self, blob_gas_used: Option<u64>) -> Self {
        self.blob_gas_used = blob_gas_used;
        self
    }

    /// Add accounts to the genesis block. If the address is already present,
    /// the account is updated.
    pub fn extend_accounts(
        mut self,
        accounts: impl IntoIterator<Item = (Address, GenesisAccount)>,
    ) -> Self {
        self.alloc.extend(accounts);
        self
    }
}

/// An account in the state of the genesis block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisAccount {
    /// The nonce of the account at genesis.
    #[serde(skip_serializing_if = "Option::is_none", with = "alloy_serde::quantity::opt", default)]
    pub nonce: Option<u64>,
    /// The balance of the account at genesis.
    pub balance: U256,
    /// The account's bytecode at genesis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<Bytes>,
    /// The account's storage at genesis.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_flagged_storage_map"
    )]
    pub storage: Option<BTreeMap<B256, FlaggedStorage>>,
    /// The account's private key. Should only be used for testing.
    #[serde(
        rename = "secretKey",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_private_key"
    )]
    pub private_key: Option<B256>,
}

impl GenesisAccount {
    /// Set the nonce.
    pub const fn with_nonce(mut self, nonce: Option<u64>) -> Self {
        self.nonce = nonce;
        self
    }

    /// Set the balance.
    pub const fn with_balance(mut self, balance: U256) -> Self {
        self.balance = balance;
        self
    }

    /// Set the code.
    pub fn with_code(mut self, code: Option<Bytes>) -> Self {
        self.code = code;
        self
    }

    /// Set the storage.
    pub fn with_storage(mut self, storage: Option<BTreeMap<B256, FlaggedStorage>>) -> Self {
        self.storage = storage;
        self
    }

    /// Returns an iterator over the storage slots in (`B256`, `U256`) format.
    pub fn storage_slots(&self) -> impl Iterator<Item = (B256, FlaggedStorage)> + '_ {
        self.storage
            .as_ref()
            .into_iter()
            .flat_map(|storage| storage.iter())
            .map(|(key, flagged_value)| (*key, *flagged_value))
    }

    /// Convert the genesis account into the [`TrieAccount`] format.
    pub fn into_trie_account(self) -> TrieAccount {
        self.into()
    }
}

impl From<GenesisAccount> for TrieAccount {
    fn from(account: GenesisAccount) -> Self {
        let storage_root = account
            .storage
            .map(|storage| {
                seismic_alloy_trie::storage_root_unhashed(
                    storage.into_iter().filter(|(_, value)| !value.is_zero()),
                )
            })
            .unwrap_or(EMPTY_ROOT_HASH);

        Self {
            nonce: account.nonce.unwrap_or_default(),
            balance: account.balance,
            storage_root,
            code_hash: account.code.map_or(KECCAK_EMPTY, keccak256),
        }
    }
}

impl From<alloy_genesis::GenesisAccount> for GenesisAccount {
    fn from(account: alloy_genesis::GenesisAccount) -> Self {
        Self {
            nonce: account.nonce,
            balance: account.balance,
            code: account.code,
            storage: match account.storage {
                Some(storage) => Some(convert_fixedbytes_map_to_flagged_storage(storage)),
                None => None,
            },
            private_key: account.private_key,
        }
    }
}

/// Custom deserialization function for the private key.
///
/// This function allows the private key to be deserialized from a string or a `null` value.
///
/// We need a custom function here especially to handle the case where the private key is `0x` and
/// should be deserialized as `None`.
fn deserialize_private_key<'de, D>(deserializer: D) -> Result<Option<B256>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt_str: Option<String> = Option::deserialize(deserializer)?;

    if let Some(ref s) = opt_str {
        if s == "0x" {
            return Ok(None);
        }
        B256::from_str(s).map(Some).map_err(D::Error::custom)
    } else {
        Ok(None)
    }
}

/// Custom deserialization function for the flagged storage map.
pub fn deserialize_flagged_storage_map<'de, D>(
    deserializer: D,
) -> Result<Option<BTreeMap<B256, FlaggedStorage>>, D::Error>
where
    D: Deserializer<'de>,
{
    let map = Option::<BTreeMap<Bytes, serde_json::Value>>::deserialize(deserializer)?;
    match map {
        Some(map) => {
            let mut res_map = BTreeMap::new();
            for (k, v) in map {
                let k_deserialized = from_bytes_to_b256::<'de, D>(k)?;

                // Handle backwards compatibility: both string and object formats
                let flagged_storage = if let serde_json::Value::String(s) = &v {
                    // Old format: simple hex string
                    let v_bytes = Bytes::from_str(s).map_err(D::Error::custom)?;
                    let v_deserialized = from_bytes_to_b256::<'de, D>(v_bytes)?;
                    FlaggedStorage { value: v_deserialized.into(), is_private: false }
                } else if let serde_json::Value::Object(obj) = &v {
                    // New format: object with value and is_private
                    let value_str = obj.get("value").and_then(|v| v.as_str()).ok_or_else(|| {
                        D::Error::custom("missing 'value' field in FlaggedStorage object")
                    })?;
                    let v_bytes = Bytes::from_str(value_str).map_err(D::Error::custom)?;
                    let v_deserialized = from_bytes_to_b256::<'de, D>(v_bytes)?;
                    let is_private =
                        obj.get("is_private").and_then(|v| v.as_bool()).unwrap_or(false);
                    FlaggedStorage { value: v_deserialized.into(), is_private }
                } else {
                    return Err(D::Error::custom("FlaggedStorage must be either a hex string or an object with 'value' and 'is_private' fields"));
                };

                res_map.insert(k_deserialized, flagged_storage);
            }
            Ok(Some(res_map))
        }
        None => Ok(None),
    }
}

/// Convert a BTreeMap<B256, B256> to BTreeMap<B256, FlaggedStorage> with is_private = false
pub fn convert_fixedbytes_map_to_flagged_storage(
    storage: BTreeMap<B256, B256>,
) -> BTreeMap<B256, FlaggedStorage> {
    storage
        .into_iter()
        .map(|(key, value)| {
            let flagged_storage =
                FlaggedStorage { value: U256::from_be_bytes(value.0), is_private: false };
            (key, flagged_storage)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::BTreeMap, vec};
    use alloy_genesis::EthashConfig;
    use alloy_primitives::{hex, Bytes, FixedBytes};
    use alloy_trie::{root::storage_root_unhashed, TrieAccount};
    use core::str::FromStr;
    use serde_json::json;

    #[test]
    fn genesis_defaults_config() {
        let s = r#"{}"#;
        let genesis: Genesis = serde_json::from_str(s).unwrap();
        assert_eq!(genesis.config.chain_id, 1);
    }

    #[test]
    fn test_genesis() {
        let default_genesis = Genesis::default();

        let nonce = 999;
        let timestamp = 12345;
        let extra_data = Bytes::from(b"extra-data");
        let gas_limit = 333333;
        let difficulty = U256::from(9000);
        let mix_hash =
            hex!("74385b512f1e0e47100907efe2b00ac78df26acba6dd16b0772923068a5801a8").into();
        let coinbase = hex!("265873b6faf3258b3ab0827805386a2a20ed040e").into();
        // create dummy account
        let first_address: Address = hex!("7618a8c597b89e01c66a1f662078992c52a30c9a").into();
        let mut account = BTreeMap::default();
        account.insert(first_address, GenesisAccount::default());

        // check values updated
        let custom_genesis = Genesis::default()
            .with_nonce(nonce)
            .with_timestamp(timestamp)
            .with_extra_data(extra_data.clone())
            .with_gas_limit(gas_limit)
            .with_difficulty(difficulty)
            .with_mix_hash(mix_hash)
            .with_coinbase(coinbase)
            .extend_accounts(account.clone());

        assert_ne!(custom_genesis, default_genesis);
        // check every field
        assert_eq!(custom_genesis.nonce, nonce);
        assert_eq!(custom_genesis.timestamp, timestamp);
        assert_eq!(custom_genesis.extra_data, extra_data);
        assert_eq!(custom_genesis.gas_limit, gas_limit);
        assert_eq!(custom_genesis.difficulty, difficulty);
        assert_eq!(custom_genesis.mix_hash, mix_hash);
        assert_eq!(custom_genesis.coinbase, coinbase);
        assert_eq!(custom_genesis.alloc, account.clone());

        // update existing account
        assert_eq!(custom_genesis.alloc.len(), 1);
        let same_address = first_address;
        let new_alloc_account = GenesisAccount {
            nonce: Some(1),
            balance: U256::from(1),
            code: Some(b"code".into()),
            storage: Some(BTreeMap::default()),
            private_key: None,
        };
        let mut updated_account = BTreeMap::default();
        updated_account.insert(same_address, new_alloc_account);
        let custom_genesis = custom_genesis.extend_accounts(updated_account.clone());
        assert_ne!(account, updated_account);
        assert_eq!(custom_genesis.alloc.len(), 1);

        // add second account
        let different_address = hex!("94e0681e3073dd71cec54b53afe988f39078fd1a").into();
        let more_accounts = BTreeMap::from([(different_address, GenesisAccount::default())]);
        let custom_genesis = custom_genesis.extend_accounts(more_accounts);
        assert_eq!(custom_genesis.alloc.len(), 2);

        // ensure accounts are different
        let first_account = custom_genesis.alloc.get(&first_address);
        let second_account = custom_genesis.alloc.get(&different_address);
        assert!(first_account.is_some());
        assert!(second_account.is_some());
        assert_ne!(first_account, second_account);
    }

    #[test]
    fn test_genesis_account() {
        let default_account = GenesisAccount::default();

        let nonce = Some(1);
        let balance = U256::from(33);
        let code = Some(b"code".into());
        let root = hex!("9474ddfcea39c5a690d2744103e39d1ff1b03d18db10fc147d970ad24699395a").into();
        let value: FixedBytes<32> =
            hex!("58eb8294d9bb16832a9dabfcb270fff99ab8ee1d8764e4f3d9fdf59ec1dee469").into();
        let value: FlaggedStorage = FlaggedStorage { value: value.into(), is_private: false };
        let mut map = BTreeMap::default();
        map.insert(root, value);
        let storage = Some(map);

        let genesis_account = GenesisAccount::default()
            .with_nonce(nonce)
            .with_balance(balance)
            .with_code(code.clone())
            .with_storage(storage.clone());

        assert_ne!(default_account, genesis_account);
        // check every field
        assert_eq!(genesis_account.nonce, nonce);
        assert_eq!(genesis_account.balance, balance);
        assert_eq!(genesis_account.code, code);
        assert_eq!(genesis_account.storage, storage);
    }

    #[test]
    fn parse_hive_genesis() {
        let geth_genesis = r#"
    {
        "difficulty": "0x20000",
        "gasLimit": "0x1",
        "alloc": {},
        "config": {
          "ethash": {},
          "chainId": 1
        }
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_hive_clique_smoke_genesis() {
        let geth_genesis = r#"
    {
      "difficulty": "0x1",
      "gasLimit": "0x400000",
      "extraData":
    "0x0000000000000000000000000000000000000000000000000000000000000000658bdf435d810c91414ec09147daa6db624063790000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    ,   "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "nonce": "0x0",
      "timestamp": "0x5c51a607",
      "alloc": {}
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_non_hex_prefixed_balance() {
        // tests that we can parse balance / difficulty fields that are either hex or decimal
        let example_balance_json = r#"
    {
        "nonce": "0x0000000000000042",
        "difficulty": "34747478",
        "mixHash": "0x123456789abcdef123456789abcdef123456789abcdef123456789abcdef1234",
        "coinbase": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "timestamp": "0x123456",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "extraData": "0xfafbfcfd",
        "gasLimit": "0x2fefd8",
        "alloc": {
            "0x3E951C9f69a06Bc3AD71fF7358DbC56bEd94b9F2": {
              "balance": "1000000000000000000000000000"
            },
            "0xe228C30d4e5245f967ac21726d5412dA27aD071C": {
              "balance": "1000000000000000000000000000"
            },
            "0xD59Ce7Ccc6454a2D2C2e06bbcf71D0Beb33480eD": {
              "balance": "1000000000000000000000000000"
            },
            "0x1CF4D54414eF51b41f9B2238c57102ab2e61D1F2": {
              "balance": "1000000000000000000000000000"
            },
            "0x249bE3fDEd872338C733cF3975af9736bdCb9D4D": {
              "balance": "1000000000000000000000000000"
            },
            "0x3fCd1bff94513712f8cD63d1eD66776A67D5F78e": {
              "balance": "1000000000000000000000000000"
            }
        },
        "config": {
            "ethash": {},
            "chainId": 10,
            "homesteadBlock": 0,
            "eip150Block": 0,
            "eip155Block": 0,
            "eip158Block": 0,
            "byzantiumBlock": 0,
            "constantinopleBlock": 0,
            "petersburgBlock": 0,
            "istanbulBlock": 0
        }
    }
    "#;

        let genesis: Genesis = serde_json::from_str(example_balance_json).unwrap();

        // check difficulty against hex ground truth
        let expected_difficulty = U256::from_str("0x2123456").unwrap();
        assert_eq!(expected_difficulty, genesis.difficulty);

        // check all alloc balances
        let dec_balance = U256::from_str("1000000000000000000000000000").unwrap();
        for alloc in &genesis.alloc {
            assert_eq!(alloc.1.balance, dec_balance);
        }
    }

    #[test]
    fn parse_hive_rpc_genesis() {
        let geth_genesis = r#"
    {
      "config": {
        "chainId": 7,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip150Hash": "0x5de1ee4135274003348e80b788e5afa4b18b18d320a5622218d5c493fedf5689",
        "eip155Block": 0,
        "eip158Block": 0
      },
      "coinbase": "0x0000000000000000000000000000000000000000",
      "difficulty": "0x20000",
      "extraData":
    "0x0000000000000000000000000000000000000000000000000000000000000000658bdf435d810c91414ec09147daa6db624063790000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    ,   "gasLimit": "0x2fefd8",
      "nonce": "0x0000000000000000",
      "timestamp": "0x1234",
      "alloc": {
        "cf49fda3be353c69b41ed96333cd24302da4556f": {
          "balance": "0x123450000000000000000"
        },
        "0161e041aad467a890839d5b08b138c1e6373072": {
          "balance": "0x123450000000000000000"
        },
        "87da6a8c6e9eff15d703fc2773e32f6af8dbe301": {
          "balance": "0x123450000000000000000"
        },
        "b97de4b8c857e4f6bc354f226dc3249aaee49209": {
          "balance": "0x123450000000000000000"
        },
        "c5065c9eeebe6df2c2284d046bfc906501846c51": {
          "balance": "0x123450000000000000000"
        },
        "0000000000000000000000000000000000000314": {
          "balance": "0x0",
          "code":
    "0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063a223e05d1461006a578063abd1a0cf1461008d578063abfced1d146100d4578063e05c914a14610110578063e6768b451461014c575b610000565b346100005761007761019d565b6040518082815260200191505060405180910390f35b34610000576100be600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919050506101a3565b6040518082815260200191505060405180910390f35b346100005761010e600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919080359060200190919050506101ed565b005b346100005761014a600480803590602001909190803573ffffffffffffffffffffffffffffffffffffffff16906020019091905050610236565b005b346100005761017960048080359060200190919080359060200190919080359060200190919050506103c4565b60405180848152602001838152602001828152602001935050505060405180910390f35b60005481565b6000600160008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205490505b919050565b80600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055505b5050565b7f6031a8d62d7c95988fa262657cd92107d90ed96e08d8f867d32f26edfe85502260405180905060405180910390a17f47e2689743f14e97f7dcfa5eec10ba1dff02f83b3d1d4b9c07b206cbbda66450826040518082815260200191505060405180910390a1817fa48a6b249a5084126c3da369fbc9b16827ead8cb5cdc094b717d3f1dcd995e2960405180905060405180910390a27f7890603b316f3509577afd111710f9ebeefa15e12f72347d9dffd0d65ae3bade81604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390a18073ffffffffffffffffffffffffffffffffffffffff167f7efef9ea3f60ddc038e50cccec621f86a0195894dc0520482abf8b5c6b659e4160405180905060405180910390a28181604051808381526020018273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019250505060405180910390a05b5050565b6000600060008585859250925092505b935093509390505600a165627a7a72305820aaf842d0d0c35c45622c5263cbb54813d2974d3999c8c38551d7c613ea2bc1170029"
    ,       "storage": {
            "0x0000000000000000000000000000000000000000000000000000000000000000": "0x1234",
            "0x6661e9d6d8b923d5bbaab1b96e1dd51ff6ea2a93520fdc9eb75d059238b8c5e9": "0x01"
          }
        },
        "0000000000000000000000000000000000000315": {
          "balance": "0x9999999999999999999999999999999",
          "code":
    "0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063ef2769ca1461003e575b610000565b3461000057610078600480803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803590602001909190505061007a565b005b8173ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051809050600060405180830381858888f1935050505015610106578173ffffffffffffffffffffffffffffffffffffffff167f30a3c50752f2552dcc2b93f5b96866280816a986c0c0408cb6778b9fa198288f826040518082815260200191505060405180910390a25b5b50505600a165627a7a72305820637991fabcc8abad4294bf2bb615db78fbec4edff1635a2647d3894e2daf6a610029"
        }
      }
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_hive_graphql_genesis() {
        let geth_genesis = r#"
    {
        "config"     : {},
        "coinbase"   : "0x8888f1f195afa192cfee860698584c030f4c9db1",
        "difficulty" : "0x020000",
        "extraData"  : "0x42",
        "gasLimit"   : "0x2fefd8",
        "mixHash"    : "0x2c85bcbce56429100b2108254bb56906257582aeafcbd682bc9af67a9f5aee46",
        "nonce"      : "0x78cc16f7b4f65485",
        "parentHash" : "0x0000000000000000000000000000000000000000000000000000000000000000",
        "timestamp"  : "0x54c98c81",
        "alloc"      : {
            "a94f5374fce5edbc8e2a8697c15331677e6ebf0b": {
                "balance" : "0x09184e72a000"
            }
        }
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_hive_engine_genesis() {
        let geth_genesis = r#"
    {
      "config": {
        "chainId": 7,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip150Hash": "0x5de1ee4135274003348e80b788e5afa4b18b18d320a5622218d5c493fedf5689",
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "muirGlacierBlock": 0,
        "berlinBlock": 0,
        "yolov2Block": 0,
        "yolov3Block": 0,
        "londonBlock": 0
      },
      "coinbase": "0x0000000000000000000000000000000000000000",
      "difficulty": "0x30000",
      "extraData":
    "0x0000000000000000000000000000000000000000000000000000000000000000658bdf435d810c91414ec09147daa6db624063790000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    ,   "gasLimit": "0x2fefd8",
      "nonce": "0x0000000000000000",
      "timestamp": "0x1234",
      "alloc": {
        "cf49fda3be353c69b41ed96333cd24302da4556f": {
          "balance": "0x123450000000000000000"
        },
        "0161e041aad467a890839d5b08b138c1e6373072": {
          "balance": "0x123450000000000000000"
        },
        "87da6a8c6e9eff15d703fc2773e32f6af8dbe301": {
          "balance": "0x123450000000000000000"
        },
        "b97de4b8c857e4f6bc354f226dc3249aaee49209": {
          "balance": "0x123450000000000000000"
        },
        "c5065c9eeebe6df2c2284d046bfc906501846c51": {
          "balance": "0x123450000000000000000"
        },
        "0000000000000000000000000000000000000314": {
          "balance": "0x0",
          "code":
    "0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063a223e05d1461006a578063abd1a0cf1461008d578063abfced1d146100d4578063e05c914a14610110578063e6768b451461014c575b610000565b346100005761007761019d565b6040518082815260200191505060405180910390f35b34610000576100be600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919050506101a3565b6040518082815260200191505060405180910390f35b346100005761010e600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919080359060200190919050506101ed565b005b346100005761014a600480803590602001909190803573ffffffffffffffffffffffffffffffffffffffff16906020019091905050610236565b005b346100005761017960048080359060200190919080359060200190919080359060200190919050506103c4565b60405180848152602001838152602001828152602001935050505060405180910390f35b60005481565b6000600160008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205490505b919050565b80600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055505b5050565b7f6031a8d62d7c95988fa262657cd92107d90ed96e08d8f867d32f26edfe85502260405180905060405180910390a17f47e2689743f14e97f7dcfa5eec10ba1dff02f83b3d1d4b9c07b206cbbda66450826040518082815260200191505060405180910390a1817fa48a6b249a5084126c3da369fbc9b16827ead8cb5cdc094b717d3f1dcd995e2960405180905060405180910390a27f7890603b316f3509577afd111710f9ebeefa15e12f72347d9dffd0d65ae3bade81604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390a18073ffffffffffffffffffffffffffffffffffffffff167f7efef9ea3f60ddc038e50cccec621f86a0195894dc0520482abf8b5c6b659e4160405180905060405180910390a28181604051808381526020018273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019250505060405180910390a05b5050565b6000600060008585859250925092505b935093509390505600a165627a7a72305820aaf842d0d0c35c45622c5263cbb54813d2974d3999c8c38551d7c613ea2bc1170029"
    ,       "storage": {
            "0x0000000000000000000000000000000000000000000000000000000000000000": "0x1234",
            "0x6661e9d6d8b923d5bbaab1b96e1dd51ff6ea2a93520fdc9eb75d059238b8c5e9": "0x01"
          }
        },
        "0000000000000000000000000000000000000315": {
          "balance": "0x9999999999999999999999999999999",
          "code":
    "0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063ef2769ca1461003e575b610000565b3461000057610078600480803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803590602001909190505061007a565b005b8173ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051809050600060405180830381858888f1935050505015610106578173ffffffffffffffffffffffffffffffffffffffff167f30a3c50752f2552dcc2b93f5b96866280816a986c0c0408cb6778b9fa198288f826040518082815260200191505060405180910390a25b5b50505600a165627a7a72305820637991fabcc8abad4294bf2bb615db78fbec4edff1635a2647d3894e2daf6a610029"
        },
        "0000000000000000000000000000000000000316": {
          "balance": "0x0",
          "code": "0x444355"
        },
        "0000000000000000000000000000000000000317": {
          "balance": "0x0",
          "code": "0x600160003555"
        }
      }
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_hive_devp2p_genesis() {
        let geth_genesis = r#"
    {
        "config": {
            "chainId": 19763,
            "homesteadBlock": 0,
            "eip150Block": 0,
            "eip155Block": 0,
            "eip158Block": 0,
            "byzantiumBlock": 0,
            "ethash": {}
        },
        "nonce": "0xdeadbeefdeadbeef",
        "timestamp": "0x0",
        "extraData": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "gasLimit": "0x80000000",
        "difficulty": "0x20000",
        "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "coinbase": "0x0000000000000000000000000000000000000000",
        "alloc": {
            "71562b71999873db5b286df957af199ec94617f7": {
                "balance": "0xffffffffffffffffffffffffff"
            }
        },
        "number": "0x0",
        "gasUsed": "0x0",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000"
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_deposit_contract_address() {
        let genesis = r#"
    {
      "config": {
        "chainId": 1337,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "muirGlacierBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "arrowGlacierBlock": 0,
        "grayGlacierBlock": 0,
        "shanghaiTime": 0,
        "cancunTime": 0,
        "pragueTime": 1,
        "osakaTime": 2,
        "terminalTotalDifficulty": 0,
        "depositContractAddress": "0x0000000000000000000000000000000000000000",
        "terminalTotalDifficultyPassed": true
      },
      "nonce": "0x0",
      "timestamp": "0x0",
      "extraData": "0x",
      "gasLimit": "0x4c4b40",
      "difficulty": "0x1",
      "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "coinbase": "0x0000000000000000000000000000000000000000"
    }
    "#;

        let got_genesis: Genesis = serde_json::from_str(genesis).unwrap();
        let expected_genesis = Genesis {
            config: ChainConfig {
                chain_id: 1337,
                homestead_block: Some(0),
                eip150_block: Some(0),
                eip155_block: Some(0),
                eip158_block: Some(0),
                byzantium_block: Some(0),
                constantinople_block: Some(0),
                petersburg_block: Some(0),
                istanbul_block: Some(0),
                muir_glacier_block: Some(0),
                berlin_block: Some(0),
                london_block: Some(0),
                arrow_glacier_block: Some(0),
                gray_glacier_block: Some(0),
                dao_fork_block: None,
                dao_fork_support: false,
                shanghai_time: Some(0),
                cancun_time: Some(0),
                prague_time: Some(1),
                osaka_time: Some(2),
                terminal_total_difficulty: Some(U256::ZERO),
                terminal_total_difficulty_passed: true,
                deposit_contract_address: Some(Address::ZERO),
                ..Default::default()
            },
            nonce: 0,
            timestamp: 0,
            extra_data: Bytes::new(),
            gas_limit: 0x4c4b40,
            difficulty: U256::from(1),
            ..Default::default()
        };

        assert_eq!(expected_genesis, got_genesis);
    }

    #[test]
    fn parse_prague_time() {
        let genesis = r#"
    {
      "config": {
        "chainId": 1337,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "muirGlacierBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "arrowGlacierBlock": 0,
        "grayGlacierBlock": 0,
        "shanghaiTime": 0,
        "cancunTime": 0,
        "pragueTime": 1,
        "terminalTotalDifficulty": 0,
        "terminalTotalDifficultyPassed": true
      },
      "nonce": "0x0",
      "timestamp": "0x0",
      "extraData": "0x",
      "gasLimit": "0x4c4b40",
      "difficulty": "0x1",
      "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "coinbase": "0x0000000000000000000000000000000000000000"
    }
    "#;

        let got_genesis: Genesis = serde_json::from_str(genesis).unwrap();
        let expected_genesis = Genesis {
            config: ChainConfig {
                chain_id: 1337,
                homestead_block: Some(0),
                eip150_block: Some(0),
                eip155_block: Some(0),
                eip158_block: Some(0),
                byzantium_block: Some(0),
                constantinople_block: Some(0),
                petersburg_block: Some(0),
                istanbul_block: Some(0),
                muir_glacier_block: Some(0),
                berlin_block: Some(0),
                london_block: Some(0),
                arrow_glacier_block: Some(0),
                gray_glacier_block: Some(0),
                dao_fork_block: None,
                dao_fork_support: false,
                shanghai_time: Some(0),
                cancun_time: Some(0),
                prague_time: Some(1),
                terminal_total_difficulty: Some(U256::ZERO),
                terminal_total_difficulty_passed: true,
                ..Default::default()
            },
            nonce: 0,
            timestamp: 0,
            extra_data: Bytes::new(),
            gas_limit: 0x4c4b40,
            difficulty: U256::from(1),
            ..Default::default()
        };

        assert_eq!(expected_genesis, got_genesis);
    }

    #[test]
    fn parse_execution_apis_genesis() {
        let geth_genesis = r#"
    {
      "config": {
        "chainId": 1337,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip150Hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "muirGlacierBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "arrowGlacierBlock": 0,
        "grayGlacierBlock": 0,
        "shanghaiTime": 0,
        "terminalTotalDifficulty": 0,
        "terminalTotalDifficultyPassed": true,
        "ethash": {}
      },
      "nonce": "0x0",
      "timestamp": "0x0",
      "extraData": "0x",
      "gasLimit": "0x4c4b40",
      "difficulty": "0x1",
      "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "coinbase": "0x0000000000000000000000000000000000000000",
      "alloc": {
        "658bdf435d810c91414ec09147daa6db62406379": {
          "balance": "0x487a9a304539440000"
        },
        "aa00000000000000000000000000000000000000": {
          "code": "0x6042",
          "storage": {
            "0x0000000000000000000000000000000000000000000000000000000000000000":
    "0x0000000000000000000000000000000000000000000000000000000000000000",
            "0x0100000000000000000000000000000000000000000000000000000000000000":
    "0x0100000000000000000000000000000000000000000000000000000000000000",
            "0x0200000000000000000000000000000000000000000000000000000000000000":
    "0x0200000000000000000000000000000000000000000000000000000000000000",
            "0x0300000000000000000000000000000000000000000000000000000000000000":
    "0x0000000000000000000000000000000000000000000000000000000000000303"       },
          "balance": "0x1",
          "nonce": "0x1"
        },
        "bb00000000000000000000000000000000000000": {
          "code": "0x600154600354",
          "storage": {
            "0x0000000000000000000000000000000000000000000000000000000000000000":
    "0x0000000000000000000000000000000000000000000000000000000000000000",
            "0x0100000000000000000000000000000000000000000000000000000000000000":
    "0x0100000000000000000000000000000000000000000000000000000000000000",
            "0x0200000000000000000000000000000000000000000000000000000000000000":
    "0x0200000000000000000000000000000000000000000000000000000000000000",
            "0x0300000000000000000000000000000000000000000000000000000000000000":
    "0x0000000000000000000000000000000000000000000000000000000000000303"       },
          "balance": "0x2",
          "nonce": "0x1"
        }
      }
    }
    "#;

        let _genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
    }

    #[test]
    fn parse_hive_rpc_genesis_full() {
        let geth_genesis = r#"
    {
      "config": {
        "clique": {
          "period": 1
        },
        "chainId": 7,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0
      },
      "coinbase": "0x0000000000000000000000000000000000000000",
      "difficulty": "0x020000",
      "extraData":
    "0x0000000000000000000000000000000000000000000000000000000000000000658bdf435d810c91414ec09147daa6db624063790000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    ,   "gasLimit": "0x2fefd8",
      "nonce": "0x0000000000000000",
      "timestamp": "0x1234",
      "alloc": {
        "cf49fda3be353c69b41ed96333cd24302da4556f": {
          "balance": "0x123450000000000000000"
        },
        "0161e041aad467a890839d5b08b138c1e6373072": {
          "balance": "0x123450000000000000000"
        },
        "87da6a8c6e9eff15d703fc2773e32f6af8dbe301": {
          "balance": "0x123450000000000000000"
        },
        "b97de4b8c857e4f6bc354f226dc3249aaee49209": {
          "balance": "0x123450000000000000000"
        },
        "c5065c9eeebe6df2c2284d046bfc906501846c51": {
          "balance": "0x123450000000000000000"
        },
        "0000000000000000000000000000000000000314": {
          "balance": "0x0",
          "code":
    "0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063a223e05d1461006a578063abd1a0cf1461008d578063abfced1d146100d4578063e05c914a14610110578063e6768b451461014c575b610000565b346100005761007761019d565b6040518082815260200191505060405180910390f35b34610000576100be600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919050506101a3565b6040518082815260200191505060405180910390f35b346100005761010e600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919080359060200190919050506101ed565b005b346100005761014a600480803590602001909190803573ffffffffffffffffffffffffffffffffffffffff16906020019091905050610236565b005b346100005761017960048080359060200190919080359060200190919080359060200190919050506103c4565b60405180848152602001838152602001828152602001935050505060405180910390f35b60005481565b6000600160008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205490505b919050565b80600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055505b5050565b7f6031a8d62d7c95988fa262657cd92107d90ed96e08d8f867d32f26edfe85502260405180905060405180910390a17f47e2689743f14e97f7dcfa5eec10ba1dff02f83b3d1d4b9c07b206cbbda66450826040518082815260200191505060405180910390a1817fa48a6b249a5084126c3da369fbc9b16827ead8cb5cdc094b717d3f1dcd995e2960405180905060405180910390a27f7890603b316f3509577afd111710f9ebeefa15e12f72347d9dffd0d65ae3bade81604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390a18073ffffffffffffffffffffffffffffffffffffffff167f7efef9ea3f60ddc038e50cccec621f86a0195894dc0520482abf8b5c6b659e4160405180905060405180910390a28181604051808381526020018273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019250505060405180910390a05b5050565b6000600060008585859250925092505b935093509390505600a165627a7a72305820aaf842d0d0c35c45622c5263cbb54813d2974d3999c8c38551d7c613ea2bc1170029"
    ,       "storage": {
            "0x0000000000000000000000000000000000000000000000000000000000000000": "0x1234",
            "0x6661e9d6d8b923d5bbaab1b96e1dd51ff6ea2a93520fdc9eb75d059238b8c5e9": "0x01"
          }
        },
        "0000000000000000000000000000000000000315": {
          "balance": "0x9999999999999999999999999999999",
          "code":
    "0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063ef2769ca1461003e575b610000565b3461000057610078600480803573ffffffffffffffffffffffffffffffffffffffff1690602001909190803590602001909190505061007a565b005b8173ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051809050600060405180830381858888f1935050505015610106578173ffffffffffffffffffffffffffffffffffffffff167f30a3c50752f2552dcc2b93f5b96866280816a986c0c0408cb6778b9fa198288f826040518082815260200191505060405180910390a25b5b50505600a165627a7a72305820637991fabcc8abad4294bf2bb615db78fbec4edff1635a2647d3894e2daf6a610029"
        }
      },
      "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
      "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000"
    }
    "#;

        let genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
        let alloc_entry = genesis
            .alloc
            .get(&Address::from_str("0000000000000000000000000000000000000314").unwrap())
            .expect("missing account for parsed genesis");
        let storage = alloc_entry.storage.as_ref().expect("missing storage for parsed genesis");
        let expected_storage = BTreeMap::from_iter(vec![
            (
                B256::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                )
                .unwrap(),
                B256::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000001234",
                )
                .unwrap(),
            ),
            (
                B256::from_str(
                    "0x6661e9d6d8b923d5bbaab1b96e1dd51ff6ea2a93520fdc9eb75d059238b8c5e9",
                )
                .unwrap(),
                B256::from_str(
                    "0x0000000000000000000000000000000000000000000000000000000000000001",
                )
                .unwrap(),
            ),
        ]);
        let expected_storage = convert_fixedbytes_map_to_flagged_storage(expected_storage);
        assert_eq!(storage, &expected_storage);

        let expected_code =
    Bytes::from_str("0x60606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063a223e05d1461006a578063abd1a0cf1461008d578063abfced1d146100d4578063e05c914a14610110578063e6768b451461014c575b610000565b346100005761007761019d565b6040518082815260200191505060405180910390f35b34610000576100be600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919050506101a3565b6040518082815260200191505060405180910390f35b346100005761010e600480803573ffffffffffffffffffffffffffffffffffffffff169060200190919080359060200190919050506101ed565b005b346100005761014a600480803590602001909190803573ffffffffffffffffffffffffffffffffffffffff16906020019091905050610236565b005b346100005761017960048080359060200190919080359060200190919080359060200190919050506103c4565b60405180848152602001838152602001828152602001935050505060405180910390f35b60005481565b6000600160008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205490505b919050565b80600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055505b5050565b7f6031a8d62d7c95988fa262657cd92107d90ed96e08d8f867d32f26edfe85502260405180905060405180910390a17f47e2689743f14e97f7dcfa5eec10ba1dff02f83b3d1d4b9c07b206cbbda66450826040518082815260200191505060405180910390a1817fa48a6b249a5084126c3da369fbc9b16827ead8cb5cdc094b717d3f1dcd995e2960405180905060405180910390a27f7890603b316f3509577afd111710f9ebeefa15e12f72347d9dffd0d65ae3bade81604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390a18073ffffffffffffffffffffffffffffffffffffffff167f7efef9ea3f60ddc038e50cccec621f86a0195894dc0520482abf8b5c6b659e4160405180905060405180910390a28181604051808381526020018273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019250505060405180910390a05b5050565b6000600060008585859250925092505b935093509390505600a165627a7a72305820aaf842d0d0c35c45622c5263cbb54813d2974d3999c8c38551d7c613ea2bc1170029"
    ).unwrap();
        let code = alloc_entry.code.as_ref().expect(
            "missing code for parsed
    genesis",
        );
        assert_eq!(code, &expected_code);
    }

    #[test]
    fn test_hive_smoke_alloc_deserialize() {
        let hive_genesis = r#"
    {
        "nonce": "0x0000000000000042",
        "difficulty": "0x2123456",
        "mixHash": "0x123456789abcdef123456789abcdef123456789abcdef123456789abcdef1234",
        "coinbase": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "timestamp": "0x123456",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "extraData": "0xfafbfcfd",
        "gasLimit": "0x2fefd8",
        "alloc": {
            "dbdbdb2cbd23b783741e8d7fcf51e459b497e4a6": {
                "balance": "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            },
            "e6716f9544a56c530d868e4bfbacb172315bdead": {
                "balance": "0x11",
                "code": "0x12"
            },
            "b9c015918bdaba24b4ff057a92a3873d6eb201be": {
                "balance": "0x21",
                "storage": {
                    "0x0000000000000000000000000000000000000000000000000000000000000001": "0x22"
                }
            },
            "1a26338f0d905e295fccb71fa9ea849ffa12aaf4": {
                "balance": "0x31",
                "nonce": "0x32"
            },
            "0000000000000000000000000000000000000001": {
                "balance": "0x41"
            },
            "0000000000000000000000000000000000000002": {
                "balance": "0x51"
            },
            "0000000000000000000000000000000000000003": {
                "balance": "0x61"
            },
            "0000000000000000000000000000000000000004": {
                "balance": "0x71"
            }
        },
        "config": {
            "ethash": {},
            "chainId": 10,
            "homesteadBlock": 0,
            "eip150Block": 0,
            "eip155Block": 0,
            "eip158Block": 0,
            "byzantiumBlock": 0,
            "constantinopleBlock": 0,
            "petersburgBlock": 0,
            "istanbulBlock": 0
        }
    }
    "#;

        let expected_genesis = Genesis {
            nonce: 0x0000000000000042,
            difficulty: U256::from(0x2123456),
            mix_hash: B256::from_str(
                "0x123456789abcdef123456789abcdef123456789abcdef123456789abcdef1234",
            )
            .unwrap(),
            coinbase: Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            timestamp: 0x123456,
            extra_data: Bytes::from_str("0xfafbfcfd").unwrap(),
            gas_limit: 0x2fefd8,
            base_fee_per_gas: None,
            excess_blob_gas: None,
            blob_gas_used: None,
            number: None,
            alloc: BTreeMap::from_iter(vec![
                (
                    Address::from_str("0xdbdbdb2cbd23b783741e8d7fcf51e459b497e4a6").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str(
                            "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                        )
                        .unwrap(),
                        nonce: None,
                        code: None,
                        storage: None,
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0xe6716f9544a56c530d868e4bfbacb172315bdead").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x11").unwrap(),
                        nonce: None,
                        code: Some(Bytes::from_str("0x12").unwrap()),
                        storage: None,
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0xb9c015918bdaba24b4ff057a92a3873d6eb201be").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x21").unwrap(),
                        nonce: None,
                        code: None,
                        storage: Some(convert_fixedbytes_map_to_flagged_storage(
                            BTreeMap::from_iter(vec![
                            (

    B256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001").
    unwrap(),
    B256::from_str("0x0000000000000000000000000000000000000000000000000000000000000022").
    unwrap(),                         ),
                        ]),
                        )),
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0x1a26338f0d905e295fccb71fa9ea849ffa12aaf4").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x31").unwrap(),
                        nonce: Some(0x32u64),
                        code: None,
                        storage: None,
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x41").unwrap(),
                        nonce: None,
                        code: None,
                        storage: None,
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0x0000000000000000000000000000000000000002").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x51").unwrap(),
                        nonce: None,
                        code: None,
                        storage: None,
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0x0000000000000000000000000000000000000003").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x61").unwrap(),
                        nonce: None,
                        code: None,
                        storage: None,
                        private_key: None,
                    },
                ),
                (
                    Address::from_str("0x0000000000000000000000000000000000000004").unwrap(),
                    GenesisAccount {
                        balance: U256::from_str("0x71").unwrap(),
                        nonce: None,
                        code: None,
                        storage: None,
                        private_key: None,
                    },
                ),
            ]),
            config: ChainConfig {
                ethash: Some(EthashConfig {}),
                chain_id: 10,
                homestead_block: Some(0),
                eip150_block: Some(0),
                eip155_block: Some(0),
                eip158_block: Some(0),
                byzantium_block: Some(0),
                constantinople_block: Some(0),
                petersburg_block: Some(0),
                istanbul_block: Some(0),
                deposit_contract_address: None,
                ..Default::default()
            },
        };

        let deserialized_genesis: Genesis = serde_json::from_str(hive_genesis).unwrap();
        assert_eq!(
            deserialized_genesis, expected_genesis,
            "deserialized genesis
    {deserialized_genesis:#?} does not match expected {expected_genesis:#?}"
        );
    }

    #[test]
    fn parse_dump_genesis_mainnet() {
        let mainnet = include_str!("../dumpgenesis/mainnet.json");
        let gen = serde_json::from_str::<Genesis>(mainnet).unwrap();
        let s = serde_json::to_string_pretty(&gen).unwrap();
        let gen2 = serde_json::from_str::<Genesis>(&s).unwrap();
        assert_eq!(gen, gen2);
    }

    #[test]
    fn parse_dump_genesis_sepolia() {
        let sepolia = include_str!("../dumpgenesis/sepolia.json");
        let gen = serde_json::from_str::<Genesis>(sepolia).unwrap();
        let s = serde_json::to_string_pretty(&gen).unwrap();
        let gen2 = serde_json::from_str::<Genesis>(&s).unwrap();
        assert_eq!(gen, gen2);
    }

    #[test]
    fn parse_dump_genesis_holesky() {
        let holesky = include_str!("../dumpgenesis/holesky.json");
        let gen = serde_json::from_str::<Genesis>(holesky).unwrap();
        let s = serde_json::to_string_pretty(&gen).unwrap();
        let gen2 = serde_json::from_str::<Genesis>(&s).unwrap();
        assert_eq!(gen, gen2);
    }

    #[test]
    fn parse_extra_fields() {
        let geth_genesis = r#"
    {
        "difficulty": "0x20000",
        "gasLimit": "0x1",
        "alloc": {},
        "config": {
          "ethash": {},
          "chainId": 1,
          "string_field": "string_value",
          "numeric_field": 7,
          "object_field": {
            "sub_field": "sub_value"
          }
        }
    }
    "#;
        let genesis: Genesis = serde_json::from_str(geth_genesis).unwrap();
        let actual_string_value = genesis.config.extra_fields.get("string_field").unwrap();
        assert_eq!(actual_string_value, "string_value");
        let actual_numeric_value = genesis.config.extra_fields.get("numeric_field").unwrap();
        assert_eq!(actual_numeric_value, 7);
        let actual_object_value = genesis.config.extra_fields.get("object_field").unwrap();
        assert_eq!(actual_object_value, &serde_json::json!({"sub_field": "sub_value"}));
    }

    #[test]
    fn deserialize_private_key_as_none_when_0x() {
        // Test case where "secretKey" is "0x", expecting None
        let json_data = json!({
            "balance": "0x0",
            "secretKey": "0x"
        });

        let account: GenesisAccount = serde_json::from_value(json_data).unwrap();
        assert_eq!(account.private_key, None);
    }

    #[test]
    fn deserialize_private_key_with_valid_hex() {
        // Test case where "secretKey" is a valid hex string
        let json_data = json!({
            "balance": "0x0",
            "secretKey": "0x123456789abcdef123456789abcdef123456789abcdef123456789abcdef1234"
        });

        let account: GenesisAccount = serde_json::from_value(json_data).unwrap();
        let expected_key =
            B256::from_str("123456789abcdef123456789abcdef123456789abcdef123456789abcdef1234")
                .unwrap();
        assert_eq!(account.private_key, Some(expected_key));
    }

    #[test]
    fn deserialize_private_key_as_none_when_null() {
        // Test case where "secretKey" is null, expecting None
        let json_data = json!({
            "balance": "0x0",
            "secretKey": null
        });

        let account: GenesisAccount = serde_json::from_value(json_data).unwrap();
        assert_eq!(account.private_key, None);
    }

    #[test]
    fn deserialize_private_key_with_invalid_hex_fails() {
        // Test case where "secretKey" is an invalid hex string, expecting an error
        let json_data = json!({
            "balance": "0x0",
            "secretKey": "0xINVALIDHEX"
        });

        let result: Result<GenesisAccount, _> = serde_json::from_value(json_data);
        assert!(result.is_err()); // The deserialization should fail due to invalid hex
    }

    #[test]
    fn deserialize_private_key_with_empty_string_fails() {
        // Test case where "secretKey" is an empty string, expecting an error
        let json_data = json!({
            "secretKey": ""
        });

        let result: Result<GenesisAccount, _> = serde_json::from_value(json_data);
        assert!(result.is_err()); // The deserialization should fail due to an empty string
    }

    #[test]
    fn test_from_genesis_account_with_default_values() {
        let genesis_account = GenesisAccount::default();

        // Convert the GenesisAccount to a TrieAccount
        let trie_account: TrieAccount = genesis_account.into();

        // Check the fields are properly set.
        assert_eq!(trie_account.nonce, 0);
        assert_eq!(trie_account.balance, U256::default());
        assert_eq!(trie_account.storage_root, EMPTY_ROOT_HASH);
        assert_eq!(trie_account.code_hash, KECCAK_EMPTY);

        // Check that the default Account converts to the same TrieAccount
        assert_eq!(TrieAccount::default(), trie_account);
    }

    #[test]
    fn test_from_genesis_account_with_values() {
        // Create a GenesisAccount with specific values
        let mut storage = BTreeMap::new();
        storage.insert(B256::from([0x01; 32]), B256::from([0x02; 32]));
        let storage = convert_fixedbytes_map_to_flagged_storage(storage);

        let genesis_account = GenesisAccount {
            nonce: Some(10),
            balance: U256::from(1000),
            code: Some(Bytes::from(vec![0x60, 0x61])),
            storage: Some(storage),
            private_key: None,
        };

        // Convert the GenesisAccount to a TrieAccount
        let trie_account: TrieAccount = genesis_account.into();

        let expected_storage_root = storage_root_unhashed(BTreeMap::from([(
            B256::from([0x01; 32]),
            U256::from_be_bytes(*B256::from([0x02; 32])),
        )]));

        // Check that the fields are properly set.
        assert_eq!(trie_account.nonce, 10);
        assert_eq!(trie_account.balance, U256::from(1000));
        assert_eq!(trie_account.storage_root, expected_storage_root);
        assert_eq!(trie_account.code_hash, keccak256([0x60, 0x61]));
    }

    #[test]
    fn test_from_genesis_account_with_zeroed_storage_values() {
        // Create a GenesisAccount with storage containing zero values
        let storage = BTreeMap::from([(B256::from([0x01; 32]), B256::from([0x00; 32]))]);
        let storage = convert_fixedbytes_map_to_flagged_storage(storage);

        let genesis_account = GenesisAccount {
            nonce: Some(3),
            balance: U256::from(300),
            code: None,
            storage: Some(storage),
            private_key: None,
        };

        // Convert the GenesisAccount to a TrieAccount
        let trie_account: TrieAccount = genesis_account.into();

        // Check the fields are properly set.
        assert_eq!(trie_account.nonce, 3);
        assert_eq!(trie_account.balance, U256::from(300));
        // Zero values in storage should result in EMPTY_ROOT_HASH
        assert_eq!(trie_account.storage_root, EMPTY_ROOT_HASH);
        // No code provided, so code hash should be KECCAK_EMPTY
        assert_eq!(trie_account.code_hash, KECCAK_EMPTY);
    }

    #[test]
    fn test_deserialize_flagged_storage_map() {
        let json_data = r#"
    {
        "nonce": "0x0000000000000042",
        "difficulty": "0x2123456",
        "mixHash": "0x123456789abcdef123456789abcdef123456789abcdef123456789abcdef1234",
        "coinbase": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "timestamp": "0x123456",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "extraData": "0xfafbfcfd",
        "gasLimit": "0x2fefd8",
        "alloc": {
            "0x1000000000000000000000000000000000000003": {
                "balance": "0x0",
                "code": "0x608060405234801561000f575f5ffd5b50600436106100fb575f3560e01c80635687f2b81161009357806395d89b411161006357806395d89b4114610218578063aa8beaf014610238578063be09129c1461024b578063f43064fb1461025e575f5ffd5b80635687f2b8146101a85780635f84a3cd146101df57806388412c20146101f25780638cdb7b3414610205575f5ffd5b8063221b7180116100ce578063221b71801461019557806323de6651146101a8578063313ce567146101bd5780633ab43673146101cc575f5ffd5b806306fdde03146100ff57806314f228031461013857806318160ddd1461015b5780632176518a1461016b575b5f5ffd5b60408051808201909152600b81526a577261707065642047617360a81b60208201525b60405161012f9190610734565b60405180910390f35b61014b610146366004610780565b610271565b604051901515815260200161012f565b5f5b60405190815260200161012f565b61017e6101793660046107aa565b6102ef565b60408051921515835260208301919091520161012f565b61015d6101a33660046107e1565b61035d565b6101bb6101b6366004610803565b505050565b005b6040516012815260200161012f565b61017e6101da3660046107e1565b6103a2565b61015d6101ed3660046107aa565b6103dc565b61014b610200366004610780565b610437565b61014b610213366004610780565b61044e565b6040805180820190915260048152635747415360e01b6020820152610122565b61014b610246366004610780565b61045b565b61014b610259366004610803565b610495565b61014b61026c366004610780565b6104b8565b335f8181526001602090815260408083206001600160a01b03871684529091528120b0909190838110156102d557604051637dc7a0d960e11b81526001600160a01b03861660048201525f6024820181905260448201526064015b60405180910390fd5b6102e282868684036104cc565b6001925050505b92915050565b5f80336001600160a01b03851681148061031a5750836001600160a01b0316816001600160a01b0316145b1561034e575050506001600160a01b038083165f9081526001602081815260408084209486168452939052919020b0610356565b5f5f92509250505b9250929050565b5f336001600160a01b0383160361038957506001600160a01b03165f908152602081905260409020b090565b60405163263e159360e11b815260040160405180910390fd5b5f80336001600160a01b038416036103d25750506001600160a01b03165f908152602081905260409020b0600191565b505f928392509050565b5f336001600160a01b0384168114806104065750826001600160a01b0316816001600160a01b0316145b156103895750506001600160a01b038083165f908152600160209081526040808320938516835292905220b06102e9565b5f336104448185856104cc565b5060019392505050565b5f3361044481858561054e565b335f8181526001602090815260408083206001600160a01b03871684529091528120b09091906102e282866104908785610855565b6104cc565b5f336104a28582856105ab565b6104ad85858561054e565b506001949350505050565b5f6104c38383610625565b50600192915050565b6001600160a01b0383166104f55760405163e602df0560e01b81525f60048201526024016102cc565b6001600160a01b03821661051e57604051634a1406b160e11b81525f60048201526024016102cc565b6001600160a01b038084165f9081526001602090815260408083209386168352929052208190b16101b683838383565b6001600160a01b03831661057757604051634b637e8f60e11b81525f60048201526024016102cc565b6001600160a01b0382166105a05760405163ec442f0560e01b81525f60048201526024016102cc565b6101b683838361065d565b6001600160a01b038084165f908152600160209081526040808320938616835292905220b05f1981101561061f578181101561061257604051637dc7a0d960e11b81526001600160a01b03841660048201525f6024820181905260448201526064016102cc565b61061f84848484036104cc565b50505050565b6001600160a01b03821661064e5760405163ec442f0560e01b81525f60048201526024016102cc565b6106595f838361065d565b5050565b6001600160a01b038316610687578060025f82825461067c9190610855565b909155506106f69050565b6001600160a01b0383165f908152602081905260409020b0818110156106d85760405163391434e360e21b81526001600160a01b03851660048201525f6024820181905260448201526064016102cc565b6001600160a01b0384165f9081526020819052604090209082900390b15b6001600160a01b03821661071257600280548290039055505050565b6001600160a01b0382165f90815260208190526040902080b0820190b1505050565b602081525f82518060208401528060208501604085015e5f604082850101526040601f19601f83011684010191505092915050565b6001600160a01b038116811461077d575f5ffd5b50565b5f5f60408385031215610791575f5ffd5b823561079c81610769565b946020939093013593505050565b5f5f604083850312156107bb575f5ffd5b82356107c681610769565b915060208301356107d681610769565b809150509250929050565b5f602082840312156107f1575f5ffd5b81356107fc81610769565b9392505050565b5f5f5f60608486031215610815575f5ffd5b833561082081610769565b9250602084013561083081610769565b929592945050506040919091013590565b634e487b7160e01b5f52601160045260245ffd5b808201808211156102e9576102e961084156fea26469706673582212204a01d1adaa4e7bd539c389daeea5cb99e1fd2f73b65a98e415c7b17605f932c764736f6c637828302e382e32382d646576656c6f702e323032352e322e31332b636f6d6d69742e39363462353035320059",
                "storage": {
                    "0x723077b8a1b173adc35e5f0e7e3662fd1208212cb629f9c128551ea7168da722": {
                        "value": "0x021e19e0c9bab2400000",
                        "is_private": true
                    }
                }
            },
            "0x1111111111111111111111111111111111111111": {
                "balance": "0x11",
                "storage": {
                    "0x0000000000000000000000000000000000000000000000000000000000000001": "0x11"
                }
            },
            "0x2222222222222222222222222222222222222222": {
                "balance": "0x22",
                "storage": {
                    "0x0000000000000000000000000000000000000000000000000000000000000002": {
                        "value": "0x22",
                        "is_private": false
                    }
                }
            }
        }
    }
        "#;

        let genesis: Genesis = serde_json::from_str(json_data).unwrap();

        // Test the first account with private storage
        let account1 = genesis
            .alloc
            .get(&Address::from_str("0x1000000000000000000000000000000000000003").unwrap())
            .unwrap();
        assert_eq!(account1.balance, U256::from(0));
        assert!(account1.code.is_some());

        let storage1 = account1.storage.as_ref().unwrap();
        let storage_key1 =
            B256::from_str("0x723077b8a1b173adc35e5f0e7e3662fd1208212cb629f9c128551ea7168da722")
                .unwrap();
        let storage_value1 = storage1.get(&storage_key1).unwrap();
        assert_eq!(storage_value1.value, U256::from_str("0x021e19e0c9bab2400000").unwrap());
        assert!(storage_value1.is_private);

        // Test the second account with simple string storage (backwards compatibility)
        let account2 = genesis
            .alloc
            .get(&Address::from_str("0x1111111111111111111111111111111111111111").unwrap())
            .unwrap();
        assert_eq!(account2.balance, U256::from(0x11));

        let storage2 = account2.storage.as_ref().unwrap();
        let storage_key2 =
            B256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        let storage_value2 = storage2.get(&storage_key2).unwrap();
        assert_eq!(storage_value2.value, U256::from(0x11));
        assert!(!storage_value2.is_private); // Default should be false for old format

        // Test the third account with explicit is_private: false
        let account3 = genesis
            .alloc
            .get(&Address::from_str("0x2222222222222222222222222222222222222222").unwrap())
            .unwrap();
        assert_eq!(account3.balance, U256::from(0x22));

        let storage3 = account3.storage.as_ref().unwrap();
        let storage_key3 =
            B256::from_str("0x0000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        let storage_value3 = storage3.get(&storage_key3).unwrap();
        assert_eq!(storage_value3.value, U256::from(0x22));
        assert!(!storage_value3.is_private);
    }
}
