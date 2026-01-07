//! Transaction metadata for AEAD encryption

use alloy_primitives::{ChainId, TxKind, U256};
use alloy_rlp::Encodable;

#[cfg(test)]
use alloy_primitives::Address;

use super::seismic::TxSeismicElements;

/// Legacy transaction fields used in metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxLegacyFields {
    /// Chain ID
    pub chain_id: ChainId,
    /// Transaction nonce
    pub nonce: u64,
    /// Gas price
    pub gas_price: u128,
    /// Gas limit
    pub gas_limit: u64,
    /// Transaction recipient or create flag
    pub to: TxKind,
    /// Transaction value
    pub value: U256,
}

impl TxLegacyFields {
    /// Encode legacy fields to a buffer
    pub fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.gas_price.encode(out);
        self.gas_limit.encode(out);
        self.to.encode(out);
        self.value.encode(out);
    }
}

/// Transaction metadata used for AEAD additional authenticated data
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxSeismicMetadata {
    /// Legacy transaction fields
    pub legacy_fields: TxLegacyFields,
    /// All seismic elements (includes security fields and encryption params)
    pub seismic_elements: TxSeismicElements,
}

impl TxSeismicMetadata {
    /// Encode the metadata as additional authenticated data for AEAD
    pub fn encode_as_aad(&self) -> Vec<u8> {
        let mut aad = Vec::new();
        // Legacy transaction fields
        self.legacy_fields.encode(&mut aad);
        // All seismic elements (includes security fields and encryption params)
        self.seismic_elements.encode(&mut aad);
        aad
    }

    #[cfg(test)]
    /// Metadata for testing
    pub fn example(seismic_elements: TxSeismicElements) -> TxSeismicMetadata {
        TxSeismicMetadata {
            legacy_fields: TxLegacyFields {
                chain_id: 5124,
                nonce: 0,
                gas_price: 7,
                gas_limit: 21000,
                to: TxKind::Call(Address::ZERO),
                value: U256::ZERO,
            },
            seismic_elements,
        }
    }
}
