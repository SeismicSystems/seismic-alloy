//! Seismic RPC network implementation
pub mod block;
pub mod builder;
pub mod envelope;
pub mod tx_request;
pub mod typed_tx;

use alloy_eip7702::constants::EIP7702_TX_TYPE_ID;
use alloy_network::{
    eip2718::{EIP1559_TX_TYPE_ID, EIP2930_TX_TYPE_ID, EIP4844_TX_TYPE_ID, LEGACY_TX_TYPE_ID},
    AnyHeader, AnyRpcHeader, AnyTxType, BuildResult, Ethereum, EthereumWallet, Network,
    NetworkWallet, TransactionBuilder, TransactionBuilderError,
};
use seismic_alloy_consensus::SEISMIC_TX_TYPE_ID;

use alloy_consensus::{
    EthereumTxEnvelope, SignableTransaction, Signed, TxEnvelope, TxType, TypedTransaction,
};
use alloy_primitives::{Address, Bytes, ChainId, TxKind, U256};
use alloy_provider::fillers::{
    BlobGasFiller, ChainIdFiller, JoinFill, NonceFiller, RecommendedFillers,
};
use alloy_rpc_types_eth::AccessList;
use alloy_serde::WithOtherFields;
use envelope::SeismicFoundryTxEnvelope;
use seismic_alloy_consensus::{
    SeismicTxEnvelope, SeismicTxType, SeismicTypedTransaction, TxSeismic,
};
use seismic_alloy_rpc_types::{SeismicTransactionReceipt, SeismicTransactionRequest};
use typed_tx::SeismicFoundryTypedTransaction;

use crate::{
    fillers::SeismicGasFiller, foundry::tx_request::SeismicFoundryRpcTransaction,
    SeismicFoundryRpcBlock,
};

/// Seismic foundry receipt response
pub type SeismicFoundryReceiptResponse = WithOtherFields<SeismicTransactionReceipt>;

/// Types for an Op-stack network.
#[derive(Clone, Copy, Debug)]
pub struct SeismicFoundry {
    _private: (),
}

impl Network for SeismicFoundry {
    type TxType = AnyTxType;

    type TxEnvelope = envelope::SeismicFoundryTxEnvelope;

    type UnsignedTx = typed_tx::SeismicFoundryTypedTransaction;

    type ReceiptEnvelope = seismic_alloy_consensus::SeismicReceiptEnvelope;

    type Header = AnyHeader;

    type TransactionRequest = tx_request::SeismicFoundryTransactionRequest;

    type TransactionResponse = SeismicFoundryRpcTransaction;

    type ReceiptResponse = SeismicFoundryReceiptResponse;

    type HeaderResponse = AnyRpcHeader;

    type BlockResponse = SeismicFoundryRpcBlock;
}

// TODO: unclear if this is correct
impl RecommendedFillers for SeismicFoundry {
    type RecommendedFillers =
        JoinFill<SeismicGasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>;

    fn recommended_fillers() -> Self::RecommendedFillers {
        Default::default()
    }
}

impl TransactionBuilder<SeismicFoundry> for SeismicTransactionRequest {
    fn chain_id(&self) -> Option<ChainId> {
        self.inner.chain_id()
    }

    fn set_chain_id(&mut self, chain_id: ChainId) {
        self.inner.set_chain_id(chain_id);
    }

    fn nonce(&self) -> Option<u64> {
        self.inner.nonce
    }

    fn set_nonce(&mut self, nonce: u64) {
        self.inner.set_nonce(nonce);
    }

    fn input(&self) -> Option<&Bytes> {
        self.inner.input.input()
    }

    fn set_input<T: Into<Bytes>>(&mut self, input: T) {
        self.inner.set_input(input);
    }

    fn from(&self) -> Option<Address> {
        self.inner.from
    }

    fn set_from(&mut self, from: Address) {
        self.inner.set_from(from);
    }

    fn kind(&self) -> Option<TxKind> {
        self.inner.kind()
    }

    fn clear_kind(&mut self) {
        self.inner.clear_kind();
    }

    fn set_kind(&mut self, kind: TxKind) {
        self.inner.set_kind(kind);
    }

    fn value(&self) -> Option<U256> {
        self.inner.value
    }

    fn set_value(&mut self, value: U256) {
        self.inner.set_value(value);
    }

    fn gas_price(&self) -> Option<u128> {
        self.inner.gas_price
    }

    fn set_gas_price(&mut self, gas_price: u128) {
        self.inner.set_gas_price(gas_price);
    }

    fn max_fee_per_gas(&self) -> Option<u128> {
        self.inner.max_fee_per_gas
    }

    fn set_max_fee_per_gas(&mut self, max_fee_per_gas: u128) {
        self.inner.set_max_fee_per_gas(max_fee_per_gas);
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.inner.max_priority_fee_per_gas
    }

    fn set_max_priority_fee_per_gas(&mut self, max_priority_fee_per_gas: u128) {
        self.inner.set_max_priority_fee_per_gas(max_priority_fee_per_gas);
    }

    fn gas_limit(&self) -> Option<u64> {
        self.inner.gas
    }

    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.inner.set_gas_limit(gas_limit);
    }

    fn access_list(&self) -> Option<&AccessList> {
        self.inner.access_list.as_ref()
    }

    fn set_access_list(&mut self, access_list: AccessList) {
        self.inner.set_access_list(access_list);
    }

    fn complete_type(&self, ty: AnyTxType) -> Result<(), Vec<&'static str>> {
        let seismic_tx_type = SeismicTxType::try_from(ty.0).map_err(|_| vec!["invalid tx type"])?;
        match seismic_tx_type {
            SeismicTxType::Seismic => self.complete_seismic(),
            _ => {
                let ty =
                    TxType::try_from(seismic_tx_type as u8).map_err(|_| vec!["invalid tx type"])?;
                self.inner.complete_type(ty)
            }
        }
    }

    fn can_submit(&self) -> bool {
        self.inner.can_submit()
    }

    fn can_build(&self) -> bool {
        let common = self.inner.gas.is_some() && self.inner.nonce.is_some();
        let seismic = self.seismic_elements.is_some();

        self.inner.can_build() || (common && seismic)
    }

    #[doc(alias = "output_transaction_type")]
    fn output_tx_type(&self) -> AnyTxType {
        match self.preferred_type() {
            SeismicTxType::Legacy => AnyTxType(LEGACY_TX_TYPE_ID),
            SeismicTxType::Eip1559 => AnyTxType(EIP1559_TX_TYPE_ID),
            SeismicTxType::Eip2930 => AnyTxType(EIP2930_TX_TYPE_ID),
            SeismicTxType::Eip4844 => AnyTxType(EIP4844_TX_TYPE_ID),
            SeismicTxType::Eip7702 => AnyTxType(EIP7702_TX_TYPE_ID),
            SeismicTxType::Seismic => AnyTxType(SEISMIC_TX_TYPE_ID),
        }
    }

    #[doc(alias = "output_transaction_type_checked")]
    fn output_tx_type_checked(&self) -> Option<AnyTxType> {
        self.buildable_type().map(|tx_ty| match tx_ty {
            SeismicTxType::Eip1559 => AnyTxType(EIP1559_TX_TYPE_ID),
            SeismicTxType::Eip2930 => AnyTxType(EIP2930_TX_TYPE_ID),
            SeismicTxType::Eip4844 => AnyTxType(EIP4844_TX_TYPE_ID),
            SeismicTxType::Eip7702 => AnyTxType(EIP7702_TX_TYPE_ID),
            SeismicTxType::Legacy => AnyTxType(LEGACY_TX_TYPE_ID),
            SeismicTxType::Seismic => AnyTxType(SEISMIC_TX_TYPE_ID),
        })
    }

    fn prep_for_submission(&mut self) {
        self.inner.prep_for_submission();
    }

    fn build_unsigned(self) -> BuildResult<SeismicFoundryTypedTransaction, SeismicFoundry> {
        if let Err((tx_type, missing)) = self.inner.missing_keys() {
            let tx_type = AnyTxType(tx_type as u8);
            return Err(TransactionBuilderError::InvalidTransactionRequest(tx_type, missing)
                .into_unbuilt(WithOtherFields::new(SeismicTransactionRequest {
                    inner: self.inner,
                    seismic_elements: self.seismic_elements,
                })));
        }
        let typed_tx = self.build_typed_tx().expect("checked by missing_keys");
        match typed_tx {
            SeismicTypedTransaction::Seismic(tx) => Ok(SeismicFoundryTypedTransaction::Seismic(tx)),
            SeismicTypedTransaction::Legacy(tx) => {
                Ok(SeismicFoundryTypedTransaction::Ethereum(TypedTransaction::Legacy(tx)))
            }
            SeismicTypedTransaction::Eip2930(tx) => {
                Ok(SeismicFoundryTypedTransaction::Ethereum(TypedTransaction::Eip2930(tx)))
            }
            SeismicTypedTransaction::Eip1559(tx) => {
                Ok(SeismicFoundryTypedTransaction::Ethereum(TypedTransaction::Eip1559(tx)))
            }
            SeismicTypedTransaction::Eip4844(tx) => {
                Ok(SeismicFoundryTypedTransaction::Ethereum(TypedTransaction::Eip4844(tx)))
            }
            SeismicTypedTransaction::Eip7702(tx) => {
                Ok(SeismicFoundryTypedTransaction::Ethereum(TypedTransaction::Eip7702(tx)))
            }
        }
    }

    async fn build<W: NetworkWallet<SeismicFoundry>>(
        self,
        wallet: &W,
    ) -> Result<<SeismicFoundry as Network>::TxEnvelope, TransactionBuilderError<SeismicFoundry>>
    {
        Ok(wallet.sign_request(WithOtherFields::new(self)).await?)
    }
}

impl From<alloy_consensus::Signed<TxSeismic>> for SeismicFoundryTxEnvelope {
    fn from(tx: Signed<TxSeismic>) -> Self {
        SeismicFoundryTxEnvelope::Seismic(tx)
    }
}

impl From<SeismicTxEnvelope> for SeismicFoundryTxEnvelope {
    fn from(value: SeismicTxEnvelope) -> Self {
        match value {
            SeismicTxEnvelope::Seismic(tx) => SeismicFoundryTxEnvelope::Seismic(tx),
            SeismicTxEnvelope::Eip1559(tx) => {
                SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip1559(tx))
            }
            SeismicTxEnvelope::Eip2930(tx) => {
                SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip2930(tx))
            }
            SeismicTxEnvelope::Eip4844(tx) => {
                SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip4844(tx))
            }
            SeismicTxEnvelope::Eip7702(tx) => {
                SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Eip7702(tx))
            }
            SeismicTxEnvelope::Legacy(tx) => {
                SeismicFoundryTxEnvelope::Ethereum(EthereumTxEnvelope::Legacy(tx))
            }
        }
    }
}

impl NetworkWallet<SeismicFoundry> for EthereumWallet {
    fn default_signer_address(&self) -> Address {
        NetworkWallet::<Ethereum>::default_signer_address(self)
    }

    fn has_signer_for(&self, address: &Address) -> bool {
        NetworkWallet::<Ethereum>::has_signer_for(self, address)
    }

    fn signer_addresses(&self) -> impl Iterator<Item = Address> {
        NetworkWallet::<Ethereum>::signer_addresses(self)
    }

    async fn sign_transaction_from(
        &self,
        sender: Address,
        tx: SeismicFoundryTypedTransaction,
    ) -> alloy_signer::Result<SeismicFoundryTxEnvelope> {
        let tx_signer = self.signer_by_address(sender).ok_or_else(|| {
            alloy_signer::Error::other(format!("Missing signing credential for {}", sender))
        })?;

        let signed_envelope = match tx {
            SeismicFoundryTypedTransaction::Seismic(mut tx) => {
                let signature = tx_signer.sign_transaction(&mut tx).await?;
                let signed_tx = tx.into_signed(signature).into();
                signed_tx
            }
            SeismicFoundryTypedTransaction::Ethereum(tx) => {
                let signed_tx =
                    NetworkWallet::<Ethereum>::sign_transaction_from(self, sender, tx).await?;
                match signed_tx {
                    TxEnvelope::Eip1559(tx) => SeismicTxEnvelope::Eip1559(tx),
                    TxEnvelope::Eip2930(tx) => SeismicTxEnvelope::Eip2930(tx),
                    TxEnvelope::Eip4844(tx) => SeismicTxEnvelope::Eip4844(tx),
                    TxEnvelope::Eip7702(tx) => SeismicTxEnvelope::Eip7702(tx),
                    TxEnvelope::Legacy(tx) => SeismicTxEnvelope::Legacy(tx),
                }
                .into()
            }
            SeismicFoundryTypedTransaction::Unknown(_) => unreachable!(),
        };
        Ok(signed_envelope)
    }
}
