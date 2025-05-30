//! Seismic RPC network implementation
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

pub use alloy_network::*;

use alloy_consensus::{SignableTransaction, TxEnvelope, TxType, TypedTransaction};
use alloy_primitives::{Address, Bytes, ChainId, TxKind, U256};
use alloy_provider::{fillers::{
    BlobGasFiller, ChainIdFiller, GasFiller, JoinFill, NonceFiller, RecommendedFillers,
}, Identity};
use alloy_rpc_types_eth::AccessList;
use seismic_alloy_consensus::{SeismicTxEnvelope, SeismicTxType, SeismicTypedTransaction};
use seismic_alloy_rpc_types::SeismicTransactionRequest;

/// Types for an Op-stack network.
#[derive(Clone, Copy, Debug)]
pub struct Seismic {
    _private: (),
}

impl Network for Seismic {
    type TxType = SeismicTxType;

    type TxEnvelope = seismic_alloy_consensus::SeismicTxEnvelope;

    type UnsignedTx = seismic_alloy_consensus::SeismicTypedTransaction;

    type ReceiptEnvelope = seismic_alloy_consensus::SeismicReceiptEnvelope;

    type Header = alloy_consensus::Header;

    type TransactionRequest = seismic_alloy_rpc_types::SeismicTransactionRequest;

    type TransactionResponse = alloy_rpc_types_eth::Transaction<SeismicTxEnvelope>;

    type ReceiptResponse = seismic_alloy_rpc_types::SeismicTransactionReceipt;

    type HeaderResponse = alloy_rpc_types_eth::Header;

    type BlockResponse =
        alloy_rpc_types_eth::Block<Self::TransactionResponse, Self::HeaderResponse>;
}

impl TransactionBuilder<Seismic> for SeismicTransactionRequest {
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

    fn complete_type(&self, ty: SeismicTxType) -> Result<(), Vec<&'static str>> {
        match ty {
            SeismicTxType::Seismic => self.complete_seismic(),
            _ => {
                let ty = TxType::try_from(ty as u8).unwrap();
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
    fn output_tx_type(&self) -> SeismicTxType {
        match self.inner.preferred_type() {
            TxType::Eip1559 | TxType::Eip4844 => SeismicTxType::Eip1559,
            TxType::Eip2930 => SeismicTxType::Eip2930,
            TxType::Eip7702 => SeismicTxType::Eip7702,
            TxType::Legacy => SeismicTxType::Legacy,
        }
    }

    #[doc(alias = "output_transaction_type_checked")]
    fn output_tx_type_checked(&self) -> Option<SeismicTxType> {
        self.inner.buildable_type().map(|tx_ty| match tx_ty {
            TxType::Eip1559 | TxType::Eip4844 => SeismicTxType::Eip1559,
            TxType::Eip2930 => SeismicTxType::Eip2930,
            TxType::Eip7702 => SeismicTxType::Eip7702,
            TxType::Legacy => SeismicTxType::Legacy,
        })
    }

    fn prep_for_submission(&mut self) {
        self.inner.prep_for_submission();
    }

    fn build_unsigned(self) -> BuildResult<SeismicTypedTransaction, Seismic> {
        if let Err((tx_type, missing)) = self.inner.missing_keys() {
            let tx_type = SeismicTxType::try_from(tx_type as u8).unwrap();
            return Err(TransactionBuilderError::InvalidTransactionRequest(tx_type, missing)
                .into_unbuilt(self));
        }
        Ok(self.build_typed_tx().expect("checked by missing_keys"))
    }

    async fn build<W: NetworkWallet<Seismic>>(
        self,
        wallet: &W,
    ) -> Result<<Seismic as Network>::TxEnvelope, TransactionBuilderError<Seismic>> {
        Ok(wallet.sign_request(self).await?)
    }
}

impl RecommendedFillers for Seismic {
    type RecommendedFillers = <alloy_network::Ethereum as RecommendedFillers>::RecommendedFillers;

    fn recommended_fillers() -> Self::RecommendedFillers {
        Default::default()
    }
}

impl NetworkWallet<Seismic> for EthereumWallet {
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
        tx: SeismicTypedTransaction,
    ) -> alloy_signer::Result<SeismicTxEnvelope> {
        if let SeismicTypedTransaction::Seismic(mut tx) = tx {
            let signature = self
                .signer_by_address(sender)
                .ok_or_else(|| {
                    alloy_signer::Error::other(format!("Missing signing credential for {}", sender))
                })?
                .sign_transaction(&mut tx)
                .await?;

            Ok(tx.into_signed(signature).into())
        } else {
            let tx = match tx {
                SeismicTypedTransaction::Legacy(tx) => TypedTransaction::Legacy(tx),
                SeismicTypedTransaction::Eip2930(tx) => TypedTransaction::Eip2930(tx),
                SeismicTypedTransaction::Eip1559(tx) => TypedTransaction::Eip1559(tx),
                SeismicTypedTransaction::Eip7702(tx) => TypedTransaction::Eip7702(tx),
                SeismicTypedTransaction::Seismic(_tx) => unreachable!(),
            };

            let tx = NetworkWallet::<Ethereum>::sign_transaction_from(self, sender, tx).await?;

            Ok(match tx {
                TxEnvelope::Eip1559(tx) => SeismicTxEnvelope::Eip1559(tx),
                TxEnvelope::Eip2930(tx) => SeismicTxEnvelope::Eip2930(tx),
                TxEnvelope::Eip7702(tx) => SeismicTxEnvelope::Eip7702(tx),
                TxEnvelope::Legacy(tx) => SeismicTxEnvelope::Legacy(tx),
                _ => unreachable!(),
            })
        }
    }
}
