use alloy_consensus::{Transaction as TransactionTrait, Typed2718};
use alloy_eip7702::SignedAuthorization;
use alloy_network_primitives::TransactionResponse;
use alloy_primitives::{Bytes, ChainId, TxKind, B256, U256};
use alloy_rpc_types_eth::{AccessList, Transaction};
use derive_more::From;
use serde::{Deserialize, Serialize};
/// Seismic RPC transaction
pub type RpcTransaction = Transaction<seismic_alloy_consensus::SeismicTxEnvelope>;

/// Seismic RPC transaction with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct AnyRpcTransaction(pub alloy_serde::WithOtherFields<RpcTransaction>);

impl TransactionTrait for AnyRpcTransaction {
    fn chain_id(&self) -> Option<ChainId> {
        self.0.inner.chain_id()
    }

    fn nonce(&self) -> u64 {
        self.0.inner.nonce()
    }

    fn gas_limit(&self) -> u64 {
        self.0.inner.gas_limit()
    }

    fn gas_price(&self) -> Option<u128> {
        self.0.inner.effective_gas_price
    }

    fn max_fee_per_gas(&self) -> u128 {
        TransactionTrait::max_fee_per_gas(&self.0.inner)
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.0.inner.max_priority_fee_per_gas()
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        self.0.inner.max_fee_per_blob_gas()
    }

    fn priority_fee_or_price(&self) -> u128 {
        self.0.inner.priority_fee_or_price()
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        self.0.inner.effective_gas_price(base_fee)
    }

    fn is_dynamic_fee(&self) -> bool {
        self.0.inner.is_dynamic_fee()
    }

    fn kind(&self) -> TxKind {
        self.0.inner.kind()
    }

    fn is_create(&self) -> bool {
        self.0.inner.is_create()
    }

    fn value(&self) -> U256 {
        self.0.inner.value()
    }

    fn input(&self) -> &Bytes {
        self.0.inner.input()
    }

    fn access_list(&self) -> Option<&AccessList> {
        self.0.inner.access_list()
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        self.0.inner.blob_versioned_hashes()
    }

    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        self.0.inner.authorization_list()
    }
}

impl Typed2718 for AnyRpcTransaction {
    fn ty(&self) -> u8 {
        self.0.inner.ty()
    }
}

impl TransactionResponse for AnyRpcTransaction {
    fn tx_hash(&self) -> alloy_primitives::TxHash {
        self.0.inner.tx_hash()
    }

    fn block_hash(&self) -> Option<alloy_primitives::BlockHash> {
        self.0.inner.block_hash
    }

    fn block_number(&self) -> Option<u64> {
        self.0.inner.block_number
    }

    fn transaction_index(&self) -> Option<u64> {
        self.0.inner.transaction_index
    }

    fn from(&self) -> alloy_primitives::Address {
        self.0.inner.from()
    }

    fn gas_price(&self) -> Option<u128> {
        self.0.inner.effective_gas_price
    }
}
