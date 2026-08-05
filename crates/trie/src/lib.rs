//! Seismic storage trie root functions.
//!
//! These replace the upstream `alloy_trie::root::storage_root*` functions with
//! versions that accept [`FlaggedStorage`] values. The `is_private` flag is
//! encoded into the leaf value bytes via `FlaggedStorage`'s RLP `Encodable` impl,
//! so no fork of `alloy-trie` is needed.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use alloy_primitives::{keccak256, FlaggedStorage, B256};
use alloy_trie::{HashBuilder, Nibbles};

/// Hashes storage keys, sorts them, and calculates the root hash of the storage trie.
///
/// Accepts any value type that converts to [`FlaggedStorage`], so both `U256` (public)
/// and `FlaggedStorage` work. See [`storage_root`] for encoding details.
pub fn storage_root_unhashed<T: Into<FlaggedStorage>>(
    storage: impl IntoIterator<Item = (B256, T)>,
) -> B256 {
    storage_root_unsorted(storage.into_iter().map(|(slot, value)| (keccak256(slot), value)))
}

/// Sorts and calculates the root hash of account storage trie.
///
/// See [`storage_root`] for encoding details.
pub fn storage_root_unsorted<T: Into<FlaggedStorage>>(
    storage: impl IntoIterator<Item = (B256, T)>,
) -> B256 {
    let mut v = Vec::from_iter(storage);
    v.sort_unstable_by_key(|(key, _)| *key);
    storage_root(v)
}

/// Calculates the root hash of account storage trie.
///
/// The `is_private` flag is encoded into the leaf value bytes via
/// `FlaggedStorage`'s RLP `Encodable` impl: public values encode identically
/// to bare `U256`, while private values append an extra `0x01` byte.
///
/// # Panics
///
/// If the items are not in sorted order.
pub fn storage_root<T: Into<FlaggedStorage>>(storage: impl IntoIterator<Item = (B256, T)>) -> B256 {
    let mut hb = HashBuilder::default();
    for (hashed_slot, value) in storage {
        let value: FlaggedStorage = value.into();
        hb.add_leaf(Nibbles::unpack(hashed_slot), alloy_rlp::encode_fixed_size(&value).as_ref());
    }
    hb.root()
}
