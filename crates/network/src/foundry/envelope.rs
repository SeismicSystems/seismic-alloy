//! Seismic Foundry transaction envelope, meant to mimic AnyTxEnvelope
use alloy_consensus::{
    error::ValueError,
    transaction::{RlpEcdsaDecodableTx, RlpEcdsaEncodableTx},
    EthereumTxEnvelope, SignableTransaction, Signed, Transaction as TransactionTrait,
    TxEip4844Variant, TxEnvelope, Typed2718,
};
use alloy_eip7702::SignedAuthorization;
use alloy_network::{
    eip2718::{Decodable2718, Encodable2718},
    AnyTxEnvelope, UnknownTxEnvelope,
};
use alloy_primitives::{Address, Bytes, ChainId, Selector, Signature, TxKind, B256, U256};
use alloy_rpc_types_eth::AccessList;
use seismic_alloy_consensus::{
    InputDecryptionElements, InputDecryptionElementsError, SeismicTxEnvelope, TxSeismic,
};

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
    /// Convert to AnyTxEnvelope.
    ///
    /// Returns an error for Seismic transactions, which have no AnyTxEnvelope
    /// representation.
    pub fn to_any_tx_envelope(&self) -> Result<AnyTxEnvelope, ValueError<&Self>> {
        match self {
            SeismicFoundryTxEnvelope::Ethereum(tx) => Ok(AnyTxEnvelope::Ethereum(tx.clone())),
            SeismicFoundryTxEnvelope::Unknown(tx) => Ok(AnyTxEnvelope::Unknown(tx.clone())),
            SeismicFoundryTxEnvelope::Seismic(_) => Err(ValueError::new_static(
                self,
                "Can't convert Seismic transaction to AnyTxEnvelope",
            )),
        }
    }

    /// Set the input of the transaction.
    ///
    /// Returns an error for unknown transaction types whose inner format is
    /// opaque.
    pub fn set_input(&mut self, input: Bytes) -> Result<(), &'static str> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => {
                let tx = tx.tx_mut();
                tx.input = input;
            }
            SeismicFoundryTxEnvelope::Ethereum(tx) => match tx {
                EthereumTxEnvelope::Eip1559(tx) => {
                    tx.tx_mut().input = input;
                }
                EthereumTxEnvelope::Eip2930(tx) => {
                    tx.tx_mut().input = input;
                }
                EthereumTxEnvelope::Eip4844(tx) => match tx.tx_mut() {
                    TxEip4844Variant::TxEip4844(tx) => {
                        tx.input = input;
                    }
                    TxEip4844Variant::TxEip4844WithSidecar(tx) => {
                        tx.tx.input = input;
                    }
                },
                EthereumTxEnvelope::Eip7702(tx) => {
                    tx.tx_mut().input = input;
                }
                EthereumTxEnvelope::Legacy(tx) => {
                    tx.tx_mut().input = input;
                }
            },
            SeismicFoundryTxEnvelope::Unknown(_) => {
                return Err("Can't set input for unknown transaction");
            }
        }
        Ok(())
    }

    /// Returns true if this is the ethereum transaction variant
    pub const fn is_ethereum(&self) -> bool {
        matches!(self, Self::Ethereum(_))
    }

    /// Returns the inner Ethereum transaction envelope, if it is an Ethereum transaction.
    /// If the transaction is not an Ethereum transaction, it is returned as an error.
    pub fn try_into_envelope(self) -> Result<TxEnvelope, ValueError<Self>> {
        match self {
            Self::Ethereum(inner) => Ok(inner),
            this => Err(ValueError::new_static(this, "unknown transaction envelope")),
        }
    }
}

impl TryFrom<SeismicFoundryTxEnvelope> for AnyTxEnvelope {
    type Error = ValueError<SeismicFoundryTxEnvelope>;

    fn try_from(value: SeismicFoundryTxEnvelope) -> Result<Self, Self::Error> {
        match value {
            SeismicFoundryTxEnvelope::Ethereum(tx) => Ok(AnyTxEnvelope::Ethereum(tx)),
            SeismicFoundryTxEnvelope::Unknown(tx) => Ok(AnyTxEnvelope::Unknown(tx)),
            v @ SeismicFoundryTxEnvelope::Seismic(_) => {
                Err(ValueError::new_static(v, "Can't convert Seismic transaction to AnyTxEnvelope"))
            }
        }
    }
}

impl Typed2718 for SeismicFoundryTxEnvelope {
    fn ty(&self) -> u8 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.ty()
        } else {
            // SAFETY: Seismic variant handled above; Ethereum/Unknown always succeed
            self.to_any_tx_envelope().expect("non-Seismic variant").ty()
        }
    }
}

impl Encodable2718 for SeismicFoundryTxEnvelope {
    fn encode_2718(&self, out: &mut dyn alloy_primitives::bytes::BufMut) {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.encode_2718(out);
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").encode_2718(out);
        }
    }
    fn encode_2718_len(&self) -> usize {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.encode_2718_len()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").encode_2718_len()
        }
    }

    fn trie_hash(&self) -> alloy_primitives::B256 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.trie_hash()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").trie_hash()
        }
    }
}

impl Decodable2718 for SeismicFoundryTxEnvelope {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> alloy_network::eip2718::Eip2718Result<Self> {
        if ty == TxSeismic::TX_TYPE {
            let tx = TxSeismic::rlp_decode_signed(buf)?;
            // Reject every signed-read seismic tx at decode time. Signed reads are an RPC
            // `eth_call`-only construct; admitting one as a state transition (call or create)
            // would let an attacker replay an intercepted signed `eth_call` payload as a real
            // write. This is sanvil's (dev-tool) network decoder, gated for dev/prod parity with
            // the same guard in reth's consensus `SeismicTransactionSigned::typed_decode`.
            if tx.tx().seismic_elements.signed_read {
                return Err(alloy_rlp::Error::Custom(
                    "signed-read seismic transactions cannot appear in blocks or the mempool",
                )
                .into());
            }
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
            self.to_any_tx_envelope().expect("non-Seismic variant").chain_id()
        }
    }

    fn nonce(&self) -> u64 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().nonce()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").nonce()
        }
    }

    fn gas_limit(&self) -> u64 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().gas_limit()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").gas_limit()
        }
    }

    /// Get `gas_price`.
    fn gas_price(&self) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().gas_price()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").gas_price()
        }
    }

    fn max_fee_per_gas(&self) -> u128 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().max_fee_per_gas()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").max_fee_per_gas()
        }
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().max_priority_fee_per_gas()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").max_priority_fee_per_gas()
        }
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().max_fee_per_blob_gas()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").max_fee_per_blob_gas()
        }
    }

    fn priority_fee_or_price(&self) -> u128 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().priority_fee_or_price()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").priority_fee_or_price()
        }
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().effective_gas_price(base_fee)
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").effective_gas_price(base_fee)
        }
    }

    fn effective_tip_per_gas(&self, base_fee: u64) -> Option<u128> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().effective_tip_per_gas(base_fee)
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").effective_tip_per_gas(base_fee)
        }
    }

    fn is_dynamic_fee(&self) -> bool {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().is_dynamic_fee()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").is_dynamic_fee()
        }
    }

    fn kind(&self) -> TxKind {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().kind()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").kind()
        }
    }

    fn is_create(&self) -> bool {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().is_create()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").is_create()
        }
    }

    fn to(&self) -> Option<Address> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().to()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").to()
        }
    }

    fn value(&self) -> U256 {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().value()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").value()
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
            self.to_any_tx_envelope().expect("non-Seismic variant").blob_count()
        }
    }

    #[inline]
    fn blob_gas_used(&self) -> Option<u64> {
        if let SeismicFoundryTxEnvelope::Seismic(tx) = self {
            tx.tx().blob_gas_used()
        } else {
            self.to_any_tx_envelope().expect("non-Seismic variant").blob_gas_used()
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
            self.to_any_tx_envelope().expect("non-Seismic variant").authorization_count()
        }
    }
}

impl<Eip4844> TryFrom<SeismicFoundryTxEnvelope> for SeismicTxEnvelope<Eip4844>
where
    Eip4844: RlpEcdsaEncodableTx
        + RlpEcdsaDecodableTx
        + Clone
        + serde::de::DeserializeOwned
        + serde::Serialize
        + SignableTransaction<Signature>,
    TxEip4844Variant: Into<Eip4844>,
{
    type Error = ValueError<SeismicFoundryTxEnvelope>;

    fn try_from(foundry_tx: SeismicFoundryTxEnvelope) -> Result<Self, Self::Error> {
        match foundry_tx {
            SeismicFoundryTxEnvelope::Seismic(tx) => Ok(SeismicTxEnvelope::Seismic(tx)),
            SeismicFoundryTxEnvelope::Ethereum(tx_envelope) => match tx_envelope {
                EthereumTxEnvelope::Eip1559(tx) => Ok(SeismicTxEnvelope::Eip1559(tx)),
                EthereumTxEnvelope::Eip2930(tx) => Ok(SeismicTxEnvelope::Eip2930(tx)),
                EthereumTxEnvelope::Eip4844(tx) => {
                    Ok(SeismicTxEnvelope::Eip4844(tx.map(|inner_tx| inner_tx.into())))
                }
                EthereumTxEnvelope::Eip7702(tx) => Ok(SeismicTxEnvelope::Eip7702(tx)),
                EthereumTxEnvelope::Legacy(tx) => Ok(SeismicTxEnvelope::Legacy(tx)),
            },
            v @ SeismicFoundryTxEnvelope::Unknown(_) => Err(ValueError::new_static(
                v,
                "Can't convert unknown transaction to SeismicTxEnvelope",
            )),
        }
    }
}

impl InputDecryptionElements for SeismicFoundryTxEnvelope {
    fn get_decryption_elements(
        &self,
    ) -> Result<seismic_alloy_consensus::TxSeismicElements, InputDecryptionElementsError> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().get_decryption_elements(),
            SeismicFoundryTxEnvelope::Ethereum(_) => {
                Err(InputDecryptionElementsError::UnsupportedTxType("Ethereum".to_string()))
            }
            SeismicFoundryTxEnvelope::Unknown(_) => {
                Err(InputDecryptionElementsError::UnsupportedTxType("Unknown".to_string()))
            }
        }
    }

    fn get_input(&self) -> Result<Bytes, InputDecryptionElementsError> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx().get_input(),
            SeismicFoundryTxEnvelope::Ethereum(tx) => Ok(tx.input().clone()),
            SeismicFoundryTxEnvelope::Unknown(tx) => Ok(tx.input().clone()),
        }
    }

    fn set_input(&mut self, data: Bytes) -> Result<(), InputDecryptionElementsError> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => tx.tx_mut().set_input(data),
            SeismicFoundryTxEnvelope::Ethereum(tx) => {
                match tx {
                    EthereumTxEnvelope::Eip1559(tx) => {
                        tx.tx_mut().input = data;
                    }
                    EthereumTxEnvelope::Eip2930(tx) => {
                        tx.tx_mut().input = data;
                    }
                    EthereumTxEnvelope::Eip4844(tx) => match tx.tx_mut() {
                        TxEip4844Variant::TxEip4844(tx) => {
                            tx.input = data;
                        }
                        TxEip4844Variant::TxEip4844WithSidecar(tx) => {
                            tx.tx.input = data;
                        }
                    },
                    EthereumTxEnvelope::Eip7702(tx) => {
                        tx.tx_mut().input = data;
                    }
                    EthereumTxEnvelope::Legacy(tx) => {
                        tx.tx_mut().input = data;
                    }
                }
                Ok(())
            }
            SeismicFoundryTxEnvelope::Unknown(_) => {
                Err(InputDecryptionElementsError::UnsupportedTxType("Unknown".to_string()))
            }
        }
    }

    fn metadata(
        &self,
        sender: Address,
    ) -> Result<seismic_alloy_consensus::TxSeismicMetadata, InputDecryptionElementsError> {
        match self {
            SeismicFoundryTxEnvelope::Seismic(tx) => Ok(tx.tx().tx_metadata(sender)),
            SeismicFoundryTxEnvelope::Ethereum(_) => {
                Err(InputDecryptionElementsError::UnsupportedTxType("Ethereum".to_string()))
            }
            SeismicFoundryTxEnvelope::Unknown(_) => {
                Err(InputDecryptionElementsError::UnsupportedTxType("Unknown".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::SignableTransaction;
    use alloy_primitives::aliases::U96;
    use seismic_alloy_consensus::TxSeismicElements;

    /// Build the EIP-2718 bytes of a signed seismic tx with the given `signed_read`/`to`.
    /// The signature is arbitrary: the decoder gates on `signed_read` before any recovery,
    /// so a dummy signature exercises the path we care about.
    fn encoded_seismic_tx(signed_read: bool, to: TxKind) -> Vec<u8> {
        let tx = TxSeismic {
            chain_id: 31337u64,
            nonce: 0,
            gas_price: 1,
            gas_limit: 21_000,
            to,
            value: U256::ZERO,
            input: Bytes::new(),
            seismic_elements: TxSeismicElements {
                encryption_nonce: U96::ZERO,
                signed_read,
                ..Default::default()
            },
            authorization_list: vec![],
        };
        let signature = Signature::new(U256::from(1u64), U256::from(1u64), false);
        let envelope = SeismicFoundryTxEnvelope::Seismic(tx.into_signed(signature));
        let mut buf = Vec::new();
        envelope.encode_2718(&mut buf);
        buf
    }

    /// The sanvil decoder must reject a signed-read seismic call tx, matching reth's
    /// consensus-decoder gate, so a replayed signed `eth_call` can't enter a block/mempool.
    #[test]
    fn decode_2718_rejects_signed_read_write() {
        let encoded = encoded_seismic_tx(true, TxKind::Call(Address::with_last_byte(1)));
        assert!(
            SeismicFoundryTxEnvelope::decode_2718(&mut &encoded[..]).is_err(),
            "sanvil decoder must reject signed-read seismic call tx"
        );
    }

    /// A signed-read create is rejected too: a create is also a state transition, so there's no
    /// legitimate signed-read create on a block/mempool ingress path.
    #[test]
    fn decode_2718_rejects_signed_read_create() {
        let encoded = encoded_seismic_tx(true, TxKind::Create);
        assert!(
            SeismicFoundryTxEnvelope::decode_2718(&mut &encoded[..]).is_err(),
            "sanvil decoder must reject signed-read seismic create tx"
        );
    }

    /// Ordinary (non-signed-read) seismic writes must still decode unaffected.
    #[test]
    fn decode_2718_accepts_non_signed_read_write() {
        let encoded = encoded_seismic_tx(false, TxKind::Call(Address::with_last_byte(1)));
        SeismicFoundryTxEnvelope::decode_2718(&mut &encoded[..])
            .expect("non-signed-read seismic write must decode");
    }
}
