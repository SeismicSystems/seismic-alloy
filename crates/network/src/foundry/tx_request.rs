//! Seismic Foundry transaction request, meant to behave like WithOtherFields<TransactionRequest>
use alloy_consensus::{EthereumTypedTransaction, Transaction as TransactionTrait, Typed2718};
use alloy_eip7702::SignedAuthorization;
use alloy_network::{BuildResult, NetworkWallet, TransactionBuilder, TransactionBuilderError};
use alloy_network_primitives::TransactionResponse;
use alloy_primitives::{Address, Bytes, ChainId, TxKind, B256, U256};
use alloy_rpc_types_eth::{AccessList, Transaction};
use alloy_serde::WithOtherFields;
use derive_more::From;
use seismic_alloy_consensus::SeismicTxEnvelope;
use seismic_alloy_rpc_types::SeismicTransactionRequest;
use serde::{Deserialize, Serialize};

use crate::foundry::{
    envelope::SeismicFoundryTxEnvelope, typed_tx::SeismicFoundryTypedTransaction, SeismicFoundry,
};

/// Seismic RPC transaction
pub type SeismicFoundryTransactionRequest = WithOtherFields<SeismicTransactionRequest>;

/// Seismic transaction
pub type SeismicTransaction = Transaction<SeismicTxEnvelope>;

/// Seismic RPC transaction with other fields
#[derive(Clone, Debug, From, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeismicFoundryRpcTransaction(pub WithOtherFields<Transaction<SeismicFoundryTxEnvelope>>);

impl SeismicFoundryRpcTransaction {
    /// Return the inner transaction.
    pub fn inner(&self) -> &Transaction<SeismicFoundryTxEnvelope> {
        &self.0.inner
    }

    /// Convert the transaction into its inner transaction.
    pub fn into_inner(self) -> Transaction<SeismicFoundryTxEnvelope> {
        self.0.into_inner()
    }
}

impl Typed2718 for SeismicFoundryRpcTransaction {
    fn ty(&self) -> u8 {
        self.0.inner.ty()
    }
}

impl TransactionTrait for SeismicFoundryRpcTransaction {
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

impl TransactionResponse for SeismicFoundryRpcTransaction {
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

impl From<SeismicFoundryTypedTransaction> for SeismicFoundryTransactionRequest {
    fn from(value: SeismicFoundryTypedTransaction) -> Self {
        match value {
            SeismicFoundryTypedTransaction::Ethereum(tx) => match tx {
                EthereumTypedTransaction::Eip4844(tx) => WithOtherFields::new(tx.into()),
                EthereumTypedTransaction::Eip7702(tx) => WithOtherFields::new(tx.into()),
                EthereumTypedTransaction::Eip1559(tx) => WithOtherFields::new(tx.into()),
                EthereumTypedTransaction::Eip2930(tx) => WithOtherFields::new(tx.into()),
                EthereumTypedTransaction::Legacy(tx) => WithOtherFields::new(tx.into()),
            },
            SeismicFoundryTypedTransaction::Unknown(_) => {
                unimplemented!("Unknown typed transaction")
            }
            SeismicFoundryTypedTransaction::Seismic(tx) => WithOtherFields::new(tx.into()),
        }
    }
}

impl From<SeismicFoundryTxEnvelope> for SeismicFoundryTransactionRequest {
    fn from(value: SeismicFoundryTxEnvelope) -> Self {
        match value {
            SeismicFoundryTxEnvelope::Ethereum(tx) => WithOtherFields::new(tx.into()),
            SeismicFoundryTxEnvelope::Unknown(_) => unimplemented!("Unknown tx envelope"),
            SeismicFoundryTxEnvelope::Seismic(tx) => WithOtherFields::new(tx.into()),
        }
    }
}

impl AsRef<SeismicFoundryTxEnvelope> for SeismicFoundryRpcTransaction {
    fn as_ref(&self) -> &SeismicFoundryTxEnvelope {
        &self.0.inner.inner
    }
}

impl TransactionBuilder<SeismicFoundry> for SeismicFoundryTransactionRequest {
    fn chain_id(&self) -> Option<ChainId> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::chain_id(&self.inner)
    }

    fn set_chain_id(&mut self, chain_id: ChainId) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_chain_id(
            &mut self.inner,
            chain_id,
        )
    }

    fn nonce(&self) -> Option<u64> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::nonce(&self.inner)
    }

    fn set_nonce(&mut self, nonce: u64) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_nonce(
            &mut self.inner,
            nonce,
        )
    }

    fn input(&self) -> Option<&Bytes> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::input(&self.inner)
    }

    fn set_input<T: Into<Bytes>>(&mut self, input: T) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_input(
            &mut self.inner,
            input,
        )
    }

    fn from(&self) -> Option<Address> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::from(&self.inner)
    }

    fn set_from(&mut self, from: Address) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_from(
            &mut self.inner,
            from,
        )
    }

    /// Get the kind of transaction.
    fn kind(&self) -> Option<TxKind> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::kind(&self.inner)
    }

    /// Clear the kind of transaction.
    fn clear_kind(&mut self) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::clear_kind(
            &mut self.inner,
        )
    }

    /// Set the kind of transaction.
    fn set_kind(&mut self, kind: TxKind) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_kind(
            &mut self.inner,
            kind,
        )
    }

    /// Get the value for the transaction.
    fn value(&self) -> Option<U256> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::value(&self.inner)
    }

    /// Set the value for the transaction.
    fn set_value(&mut self, value: U256) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_value(
            &mut self.inner,
            value,
        )
    }

    /// Get the legacy gas price for the transaction.
    fn gas_price(&self) -> Option<u128> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::gas_price(&self.inner)
    }

    /// Set the legacy gas price for the transaction.
    fn set_gas_price(&mut self, gas_price: u128) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_gas_price(
            &mut self.inner,
            gas_price,
        )
    }

    /// Get the max fee per gas for the transaction.
    fn max_fee_per_gas(&self) -> Option<u128> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::max_fee_per_gas(
            &self.inner,
        )
    }

    /// Set the max fee per gas  for the transaction.
    fn set_max_fee_per_gas(&mut self, max_fee_per_gas: u128) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_max_fee_per_gas(
            &mut self.inner,
            max_fee_per_gas,
        )
    }

    /// Get the max priority fee per gas for the transaction.
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::max_priority_fee_per_gas(
            &self.inner,
        )
    }

    /// Set the max priority fee per gas for the transaction.
    fn set_max_priority_fee_per_gas(&mut self, max_priority_fee_per_gas: u128) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_max_priority_fee_per_gas(&mut self.inner, max_priority_fee_per_gas)
    }

    /// Get the gas limit for the transaction.
    fn gas_limit(&self) -> Option<u64> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::gas_limit(&self.inner)
    }

    /// Set the gas limit for the transaction.
    fn set_gas_limit(&mut self, gas_limit: u64) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_gas_limit(
            &mut self.inner,
            gas_limit,
        )
    }

    /// Get the EIP-2930 access list for the transaction.
    fn access_list(&self) -> Option<&AccessList> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::access_list(&self.inner)
    }

    /// Sets the EIP-2930 access list.
    fn set_access_list(&mut self, access_list: AccessList) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::set_access_list(
            &mut self.inner,
            access_list,
        )
    }

    /// Check if all necessary keys are present to build the specified type,
    /// returning a list of missing keys.
    fn complete_type(
        &self,
        ty: <SeismicFoundry as alloy_network::Network>::TxType,
    ) -> Result<(), Vec<&'static str>> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::complete_type(
            &self.inner,
            ty,
        )
    }

    /// True if the builder contains all necessary information to be submitted
    /// to the `eth_sendTransaction` endpoint.
    fn can_submit(&self) -> bool {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::can_submit(&self.inner)
    }

    /// True if the builder contains all necessary information to be built into
    /// a valid transaction.
    fn can_build(&self) -> bool {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::can_build(&self.inner)
    }

    /// Returns the transaction type that this builder will attempt to build.
    /// This does not imply that the builder is ready to build.
    #[doc(alias = "output_transaction_type")]
    fn output_tx_type(&self) -> <SeismicFoundry as alloy_network::Network>::TxType {
        self.inner.transaction_type;
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::output_tx_type(
            &self.inner,
        )
    }

    /// Returns the transaction type that this builder will build. `None` if
    /// the builder is not ready to build.
    #[doc(alias = "output_transaction_type_checked")]
    fn output_tx_type_checked(&self) -> Option<<SeismicFoundry as alloy_network::Network>::TxType> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::output_tx_type_checked(
            &self.inner,
        )
    }

    /// Trim any conflicting keys and populate any computed fields (like blob
    /// hashes).
    ///
    /// This is useful for transaction requests that have multiple conflicting
    /// fields. While these may be buildable, they may not be submitted to the
    /// RPC. This method should be called before RPC submission, but is not
    /// necessary before building.
    fn prep_for_submission(&mut self) {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::prep_for_submission(
            &mut self.inner,
        )
    }

    /// Build an unsigned, but typed, transaction.
    fn build_unsigned(
        self,
    ) -> BuildResult<<SeismicFoundry as alloy_network::Network>::UnsignedTx, SeismicFoundry> {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::build_unsigned(
            self.inner,
        )
    }

    /// Build a signed transaction.
    async fn build<W: NetworkWallet<SeismicFoundry>>(
        self,
        wallet: &W,
    ) -> Result<
        <SeismicFoundry as alloy_network::Network>::TxEnvelope,
        TransactionBuilderError<SeismicFoundry>,
    > {
        <SeismicTransactionRequest as TransactionBuilder<SeismicFoundry>>::build(self.inner, wallet)
            .await
    }
}

impl From<SeismicFoundryRpcTransaction> for SeismicFoundryTxEnvelope {
    fn from(value: SeismicFoundryRpcTransaction) -> Self {
        value.0.inner.inner.into_inner()
    }
}
