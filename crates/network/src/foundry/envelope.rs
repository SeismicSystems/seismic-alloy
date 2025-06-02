//! Seismic Foundry transaction envelope, meant to mimic AnyTxEnvelope
use alloy_consensus::{
    transaction::RlpEcdsaDecodableTx, EthereumTxEnvelope, Signed, Transaction as TransactionTrait,
    TxEnvelope, Typed2718,
};
use alloy_eip7702::SignedAuthorization;
use alloy_network::{
    eip2718::{Decodable2718, Encodable2718},
    AnyTxEnvelope, UnknownTxEnvelope,
};
use alloy_primitives::{Address, Bytes, ChainId, Selector, TxKind, B256, U256};
use alloy_rpc_types_eth::AccessList;
use seismic_alloy_consensus::{SeismicTxEnvelope, TxSeismic};

/// Seismic Foundry transaction envelope, meant to mimic AnyTxEnvelope
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum SeismicFoundryTxEnvelope {
    /// An Ethereum transaction.
    Ethereum(TxEnvelope),
    /// A transaction with unknown type.
    Unknown(UnknownTxEnvelope),
    /// A Seismic transaction.
    Seismic(Signed<TxSeismic>),
}

impl SeismicFoundryTxEnvelope {
    /// Convert to AnyTxEnvelope
    pub fn to_any_tx_envelope(&self) -> AnyTxEnvelope {
        match self {
            SeismicFoundryTxEnvelope::Ethereum(tx) => AnyTxEnvelope::Ethereum(tx.clone()),
            SeismicFoundryTxEnvelope::Unknown(tx) => AnyTxEnvelope::Unknown(tx.clone()),
            SeismicFoundryTxEnvelope::Seismic(_) => {
                panic!("Can't convert Seismic transaction to AnyTxEnvelope")
            }
        }
    }
}

impl From<SeismicFoundryTxEnvelope> for AnyTxEnvelope {
    fn from(value: SeismicFoundryTxEnvelope) -> Self {
        value.to_any_tx_envelope()
    }
}

impl Typed2718 for SeismicFoundryTxEnvelope {
    fn ty(&self) -> u8 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.ty()
        } else {
            self.to_any_tx_envelope().ty()
        }
    }
}

impl Encodable2718 for SeismicFoundryTxEnvelope {
    fn encode_2718(&self, out: &mut dyn alloy_primitives::bytes::BufMut) {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.encode_2718(out);
        } else {
            self.to_any_tx_envelope().encode_2718(out);
        }
    }
    fn encode_2718_len(&self) -> usize {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.encode_2718_len()
        } else {
            self.to_any_tx_envelope().encode_2718_len()
        }
    }

    fn trie_hash(&self) -> alloy_primitives::B256 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.trie_hash()
        } else {
            self.to_any_tx_envelope().trie_hash()
        }
    }
}

// impl Encodable for SeismicFoundryTxEnvelope {
//     fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
//         self.network_encode(out)
//     }

//     fn length(&self) -> usize {
//         self.network_len()
//     }
// }

// impl Decodable for SeismicFoundryTxEnvelope {
//     fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
//         Ok(Self::network_decode(buf)?)
//     }
// }

impl Decodable2718 for SeismicFoundryTxEnvelope {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> alloy_network::eip2718::Eip2718Result<Self> {
        if ty == TxSeismic::TX_TYPE {
            let tx = TxSeismic::rlp_decode_signed(buf)?;
            Ok(SeismicFoundryTxEnvelope::Seismic(tx))
        } else {
            let tx = AnyTxEnvelope::typed_decode(ty, buf)?;
            match tx {
                AnyTxEnvelope::Ethereum(tx) => Ok(SeismicFoundryTxEnvelope::Ethereum(tx)),
                AnyTxEnvelope::Unknown(tx) => Ok(SeismicFoundryTxEnvelope::Unknown(tx)),
            }
        }
    }

    fn fallback_decode(buf: &mut &[u8]) -> alloy_network::eip2718::Eip2718Result<Self> {
        TxEnvelope::fallback_decode(buf).map(Self::Ethereum)
    }
}

impl TransactionTrait for SeismicFoundryTxEnvelope {
    fn chain_id(&self) -> Option<ChainId> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().chain_id()
        } else {
            self.to_any_tx_envelope().chain_id()
        }
    }

    fn nonce(&self) -> u64 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().nonce()
        } else {
            self.to_any_tx_envelope().nonce()
        }
    }

    fn gas_limit(&self) -> u64 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().gas_limit()
        } else {
            self.to_any_tx_envelope().gas_limit()
        }
    }

    /// Get `gas_price`.
    fn gas_price(&self) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().gas_price()
        } else {
            self.to_any_tx_envelope().gas_price()
        }
    }

    fn max_fee_per_gas(&self) -> u128 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().max_fee_per_gas()
        } else {
            self.to_any_tx_envelope().max_fee_per_gas()
        }
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().max_priority_fee_per_gas()
        } else {
            self.to_any_tx_envelope().max_priority_fee_per_gas()
        }
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().max_fee_per_blob_gas()
        } else {
            self.to_any_tx_envelope().max_fee_per_blob_gas()
        }
    }

    fn priority_fee_or_price(&self) -> u128 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().priority_fee_or_price()
        } else {
            self.to_any_tx_envelope().priority_fee_or_price()
        }
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().effective_gas_price(base_fee)
        } else {
            self.to_any_tx_envelope().effective_gas_price(base_fee)
        }
    }

    fn effective_tip_per_gas(&self, base_fee: u64) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().effective_tip_per_gas(base_fee)
        } else {
            self.to_any_tx_envelope().effective_tip_per_gas(base_fee)
        }
    }

    fn is_dynamic_fee(&self) -> bool {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().is_dynamic_fee()
        } else {
            self.to_any_tx_envelope().is_dynamic_fee()
        }
    }

    fn kind(&self) -> TxKind {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().kind()
        } else {
            self.to_any_tx_envelope().kind()
        }
    }

    fn is_create(&self) -> bool {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().is_create()
        } else {
            self.to_any_tx_envelope().is_create()
        }
    }

    fn to(&self) -> Option<Address> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().to()
        } else {
            self.to_any_tx_envelope().to()
        }
    }

    fn value(&self) -> U256 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().value()
        } else {
            self.to_any_tx_envelope().value()
        }
    }

    fn input(&self) -> &Bytes {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().input(),
            SeismicFoundryTxEnvelope::Ethereum(tx) => tx.input(),
            SeismicFoundryTxEnvelope::Unknown(tx) => tx.input(),
        }
    }

    fn function_selector(&self) -> Option<&Selector> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().function_selector(),
            SeismicFoundryTxEnvelope::Ethereum(tx) => tx.function_selector(),
            SeismicFoundryTxEnvelope::Unknown(tx) => tx.function_selector(),
        }
    }

    fn access_list(&self) -> Option<&AccessList> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().access_list(),
            SeismicFoundryTxEnvelope::Ethereum(tx) => tx.access_list(),
            SeismicFoundryTxEnvelope::Unknown(tx) => tx.access_list(),
        }
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().blob_versioned_hashes(),
            SeismicFoundryTxEnvelope::Ethereum(tx) => tx.blob_versioned_hashes(),
            SeismicFoundryTxEnvelope::Unknown(tx) => tx.blob_versioned_hashes(),
        }
    }

    fn blob_count(&self) -> Option<u64> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().blob_count()
        } else {
            self.to_any_tx_envelope().blob_count()
        }
    }

    #[inline]
    fn blob_gas_used(&self) -> Option<u64> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().blob_gas_used()
        } else {
            self.to_any_tx_envelope().blob_gas_used()
        }
    }

    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().authorization_list(),
            SeismicFoundryTxEnvelope::Ethereum(tx) => tx.authorization_list(),
            SeismicFoundryTxEnvelope::Unknown(tx) => tx.authorization_list(),
        }
    }

    fn authorization_count(&self) -> Option<u64> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().authorization_count()
        } else {
            self.to_any_tx_envelope().authorization_count()
        }
    }
}

impl From<SeismicFoundryTxEnvelope> for SeismicTxEnvelope {
    fn from(foundry_tx: SeismicFoundryTxEnvelope) -> Self {
        match foundry_tx {
            SeismicFoundryTxEnvelope::Seismic(tx) => SeismicTxEnvelope::Seismic(tx),
            SeismicFoundryTxEnvelope::Ethereum(tx_envelope) => match tx_envelope {
                EthereumTxEnvelope::Eip1559(tx) => SeismicTxEnvelope::Eip1559(tx),
                EthereumTxEnvelope::Eip2930(tx) => SeismicTxEnvelope::Eip2930(tx),
                EthereumTxEnvelope::Eip4844(tx) => SeismicTxEnvelope::Eip4844(tx),
                EthereumTxEnvelope::Eip7702(tx) => SeismicTxEnvelope::Eip7702(tx),
                EthereumTxEnvelope::Legacy(tx) => SeismicTxEnvelope::Legacy(tx),
            },
            SeismicFoundryTxEnvelope::Unknown(_) => unimplemented!(),
        }
    }
}

/*
impl AsRef<SeismicFoundryTxEnvelope> for alloy_rpc_types_eth::Transaction<SeismicTxEnvelope> {
    fn as_ref(&self) -> &SeismicFoundryTxEnvelope {
        &self.inner().inner()
        // match self.inner.inner() {
        //     SeismicTxEnvelope::Seismic(tx) => &SeismicFoundryTxEnvelope::Seismic(tx),
        //     // SeismicTxEnvelope::Eip1559(tx) => {
        //     //     &SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip1559(tx.clone()))
        //     // }
        //     // SeismicTxEnvelope::Eip2930(tx) => {
        //     //     &SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip2930(tx.clone()))
        //     // }
        //     // SeismicTxEnvelope::Eip4844(tx) => {
        //     //     &SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip4844(tx.clone()))
        //     // }
        //     // SeismicTxEnvelope::Eip7702(tx) => {
        //     //     &SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip7702(tx.clone()))
        //     // }
        //     // SeismicTxEnvelope::Legacy(tx) => {
        //     //     &SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Legacy(tx.clone()))
        //     // }
        //     _ => unimplemented!(),
        // }
    }
}


impl AsRef<SeismicTxEnvelope> for SeismicFoundryTxEnvelope {
    fn as_ref(&self) -> &SeismicTxEnvelope {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => &SeismicTxEnvelope::Seismic(*tx),
            SeismicFoundryTxEnvelope::Ethereum(tx) => match tx {
                EthereumTxEnvelope::Eip1559(tx) => &SeismicTxEnvelope::Eip1559(*tx),
                EthereumTxEnvelope::Eip2930(tx) => &SeismicTxEnvelope::Eip2930(*tx),
                EthereumTxEnvelope::Eip4844(tx) => &SeismicTxEnvelope::Eip4844(*tx),
                EthereumTxEnvelope::Eip7702(tx) => &SeismicTxEnvelope::Eip7702(*tx),
                EthereumTxEnvelope::Legacy(tx) => &SeismicTxEnvelope::Legacy(*tx),
            },
            SeismicFoundryTxEnvelope::Unknown(tx) => unimplemented!(),
        }
    }
}
 */
