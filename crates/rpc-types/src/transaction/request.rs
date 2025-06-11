use core::fmt::Error;

use alloc::vec::Vec;
use alloy_consensus::{
    EthereumTxEnvelope, SignableTransaction, Signed, TxEip1559, TxEip2930, TxEip4844Variant,
    TxEip7702, TxLegacy, TypedTransaction,
};
use alloy_eips::{eip7702::SignedAuthorization, Typed2718};
use alloy_network_primitives::{TransactionBuilder4844, TransactionBuilder7702};
use alloy_primitives::{Address, Signature, TxKind, U256};
use alloy_rpc_types_eth::{AccessList, TransactionInput, TransactionRequest};
use alloy_serde::WithOtherFields;
use seismic_alloy_consensus::{
    Decodable712, Eip712Result, InputDecryptionElements, InputDecryptionElementsError,
    SeismicTxEnvelope, SeismicTypedTransaction, TxSeismic, TxSeismicElements, TypedDataRequest,
};
use seismic_enclave::EnclaveClient;

/// Builder for [`SeismicTypedTransaction`].
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    derive_more::From,
    derive_more::AsRef,
    derive_more::AsMut,
)]
#[cfg_attr(any(test, feature = "arbitrary"), derive(arbitrary::Arbitrary))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct SeismicTransactionRequest {
    /// The inner [`TransactionRequest`]
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub inner: TransactionRequest,

    /// Seismic-specific elements to be included in the transaction
    /// For now just encrypted call data
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub seismic_elements: Option<TxSeismicElements>,
}

impl SeismicTransactionRequest {
    /// Sets the `from` field in the call to the provided address
    #[inline]
    pub const fn from(mut self, from: Address) -> Self {
        self.inner.from = Some(from);
        self
    }

    /// Initializes the [`TransactionRequest`] with the provided transaction.
    ///
    /// Note: This leaves the `from` field empty.
    pub fn from_transaction<T: alloy_consensus::Transaction>(tx: T) -> Self {
        let inner = TransactionRequest::from_transaction(tx);
        Self { inner, seismic_elements: None }
    }

    /// Sets the transactions type for the transactions.
    #[doc(alias = "tx_type")]
    pub const fn transaction_type(mut self, transaction_type: u8) -> Self {
        self.inner.transaction_type = Some(transaction_type);
        self
    }

    /// Sets the gas limit for the transaction.
    pub const fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.inner.gas = Some(gas_limit);
        self
    }

    /// Sets the nonce for the transaction.
    pub const fn nonce(mut self, nonce: u64) -> Self {
        self.inner.nonce = Some(nonce);
        self
    }

    /// Sets the maximum fee per gas for the transaction.
    pub const fn max_fee_per_gas(mut self, max_fee_per_gas: u128) -> Self {
        self.inner.max_fee_per_gas = Some(max_fee_per_gas);
        self
    }

    /// Sets the maximum priority fee per gas for the transaction.
    pub const fn max_priority_fee_per_gas(mut self, max_priority_fee_per_gas: u128) -> Self {
        self.inner.max_priority_fee_per_gas = Some(max_priority_fee_per_gas);
        self
    }

    /// Sets the recipient address for the transaction.
    #[inline]
    pub const fn to(mut self, to: Address) -> Self {
        self.inner.to = Some(TxKind::Call(to));
        self
    }

    /// Sets the value (amount) for the transaction.
    pub const fn value(mut self, value: U256) -> Self {
        self.inner.value = Some(value);
        self
    }

    /// Sets the access list for the transaction.
    pub fn access_list(mut self, access_list: AccessList) -> Self {
        self.inner.access_list = Some(access_list);
        self
    }

    /// Sets the input data for the transaction.
    pub fn input(mut self, input: TransactionInput) -> Self {
        self.inner.input = input;
        self
    }

    /// Sets the seismic elements for the transaction, returning a new request
    pub fn seismic_elements(mut self, seismic_elements: TxSeismicElements) -> Self {
        self.seismic_elements = Some(seismic_elements);
        self
    }

    /// Sets the seismic elements for the transaction
    pub fn set_seismic_elements(&mut self, seismic_elements: TxSeismicElements) {
        self.seismic_elements = Some(seismic_elements);
    }

    fn check_seismic_fields(&self, missing: &mut Vec<&'static str>) {
        if self.inner.gas_price.is_none() {
            missing.push("gas_price");
        }
        if self.inner.chain_id.is_none() {
            missing.push("chain_id");
        }
        if self.seismic_elements.is_none() {
            missing.push("seismic_elements");
        }
        if self.inner.nonce.is_none() {
            missing.push("nonce");
        }
        if self.inner.gas.is_none() {
            missing.push("gas_limit");
        }
        if self.inner.to.is_none() {
            missing.push("to");
        }
    }

    /// Check if all necessary keys are present to build a seismic transaction,
    /// returning a list of keys that are missing.
    pub fn complete_seismic(&self) -> Result<(), Vec<&'static str>> {
        let mut missing = Vec::new();
        self.check_seismic_fields(&mut missing);

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// Build a seismic transaction.
    ///
    /// Returns an error if required fields are missing.
    /// Use `complete_seismic` to check if the request can be built.
    fn build_seismic(self) -> Result<TxSeismic, &'static str> {
        let checked_to = self.inner.to.ok_or("Missing 'to' field for seismic transaction.")?;

        Ok(TxSeismic {
            chain_id: self
                .inner
                .chain_id
                .ok_or("Missing 'chain_id' field for seismic transaction.")?,
            nonce: self.inner.nonce.ok_or("Missing 'nonce' field for seismic transaction.")?,
            gas_price: self
                .inner
                .gas_price
                .ok_or("Missing 'gas_price' for seismic transaction.")?,
            gas_limit: self.inner.gas.ok_or("Missing 'gas_limit' for seismic transaction.")?,
            to: checked_to,
            value: self.inner.value.unwrap_or_default(),
            input: self.inner.input.into_input().unwrap_or_default(),
            seismic_elements: self
                .seismic_elements
                .ok_or("Missing 'seismic_elements' for seismic transaction.")?,
        })
    }

    /// Builds [`SeismicTypedTransaction`] from this builder. See
    /// [`TransactionRequest::build_typed_tx`] for more info.
    ///
    /// Note that EIP-4844 transactions are not supported by Seismic and will be converted into
    /// EIP-1559 transactions.
    pub fn build_typed_tx(self) -> Result<SeismicTypedTransaction, Self> {
        if self.seismic_elements.is_some() {
            let tx = self.build_seismic().expect("Failed to build seismic transaction.");
            return Ok(SeismicTypedTransaction::Seismic(tx));
        }

        let tx = self
            .inner
            .build_typed_tx()
            .map_err(|orig_tx| Self { inner: orig_tx, seismic_elements: self.seismic_elements })?;

        match tx {
            TypedTransaction::Legacy(tx) => Ok(SeismicTypedTransaction::Legacy(tx)),
            TypedTransaction::Eip1559(tx) => Ok(SeismicTypedTransaction::Eip1559(tx)),
            TypedTransaction::Eip2930(tx) => Ok(SeismicTypedTransaction::Eip2930(tx)),
            TypedTransaction::Eip4844(tx) => Ok(SeismicTypedTransaction::Eip4844(tx)),
            TypedTransaction::Eip7702(tx) => Ok(SeismicTypedTransaction::Eip7702(tx)),
        }
    }

    /// Initializes the [`SeismicTransactionRequest`] with the provided transaction and sender.
    pub fn from_transaction_with_sender<T: alloy_consensus::Transaction>(
        tx: T,
        from: Address,
    ) -> Self {
        Self::from_transaction(tx).from(from)
    }

    /// Decrypts the seismic elements and returns a [`TransactionRequest`].
    pub fn to_transaction_request(
        &self,
        enclave_client: &EnclaveClient,
    ) -> Result<TransactionRequest, Error> {
        if let Some(seismic_elements) = &self.seismic_elements {
            let ciphertext = self.inner.input.input().unwrap();
            let plaintext = seismic_elements.server_decrypt(enclave_client, ciphertext).unwrap();
            self.inner.clone().input(plaintext.into());
        }
        Ok(self.inner.clone())
    }
}

impl core::ops::Deref for SeismicTransactionRequest {
    type Target = TransactionRequest;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl core::ops::DerefMut for SeismicTransactionRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<TransactionRequest> for SeismicTransactionRequest {
    fn from(tx: TransactionRequest) -> Self {
        Self { inner: tx, seismic_elements: None }
    }
}

impl From<TxLegacy> for SeismicTransactionRequest {
    fn from(tx: TxLegacy) -> Self {
        let inner = tx.into();
        Self { inner, seismic_elements: None }
    }
}

impl From<TxEip2930> for SeismicTransactionRequest {
    fn from(tx: TxEip2930) -> Self {
        let inner = tx.into();
        Self { inner, seismic_elements: None }
    }
}

impl From<TxEip1559> for SeismicTransactionRequest {
    fn from(tx: TxEip1559) -> Self {
        let inner = tx.into();
        Self { inner, seismic_elements: None }
    }
}

impl From<TxEip7702> for SeismicTransactionRequest {
    fn from(tx: TxEip7702) -> Self {
        let inner = tx.into();
        Self { inner, seismic_elements: None }
    }
}

impl From<TxEip4844Variant> for SeismicTransactionRequest {
    fn from(tx: TxEip4844Variant) -> Self {
        let inner = tx.into();
        Self { inner, seismic_elements: None }
    }
}

impl From<TxSeismic> for SeismicTransactionRequest {
    fn from(tx: TxSeismic) -> Self {
        let ty = tx.ty();
        let TxSeismic { chain_id, nonce, gas_price, gas_limit, to, value, input, seismic_elements } =
            tx;

        let inner = TransactionRequest {
            to: Some(to.into()),
            gas_price: Some(gas_price),
            gas: Some(gas_limit),
            value: Some(value),
            input: input.into(),
            nonce: Some(nonce),
            chain_id: Some(chain_id),
            transaction_type: Some(ty),
            ..Default::default()
        };

        Self { inner, seismic_elements: Some(seismic_elements) }
    }
}

impl<T> From<Signed<T, Signature>> for SeismicTransactionRequest
where
    T: SignableTransaction<Signature> + Into<SeismicTransactionRequest>,
{
    fn from(value: Signed<T, Signature>) -> Self {
        #[cfg(feature = "k256")]
        let from = value.recover_signer().ok();
        #[cfg(not(feature = "k256"))]
        let from = None;

        let mut inner: SeismicTransactionRequest = value.strip_signature().into();
        inner.inner.from = from;

        inner
    }
}

impl From<SeismicTypedTransaction> for SeismicTransactionRequest {
    fn from(tx: SeismicTypedTransaction) -> Self {
        match tx {
            SeismicTypedTransaction::Legacy(tx) => {
                Self { inner: tx.into(), seismic_elements: None }
            }
            SeismicTypedTransaction::Eip2930(tx) => {
                Self { inner: tx.into(), seismic_elements: None }
            }
            SeismicTypedTransaction::Eip1559(tx) => {
                Self { inner: tx.into(), seismic_elements: None }
            }
            SeismicTypedTransaction::Eip4844(tx) => {
                Self { inner: tx.into(), seismic_elements: None }
            }
            SeismicTypedTransaction::Eip7702(tx) => {
                Self { inner: tx.into(), seismic_elements: None }
            }
            SeismicTypedTransaction::Seismic(tx) => tx.into(),
        }
    }
}

impl From<SeismicTxEnvelope> for SeismicTransactionRequest {
    fn from(value: SeismicTxEnvelope) -> Self {
        match value {
            SeismicTxEnvelope::Legacy(tx) => tx.into(),
            SeismicTxEnvelope::Eip1559(tx) => tx.into(),
            SeismicTxEnvelope::Eip2930(tx) => tx.into(),
            SeismicTxEnvelope::Eip4844(tx) => tx.into(),
            SeismicTxEnvelope::Eip7702(tx) => tx.into(),
            SeismicTxEnvelope::Seismic(tx) => tx.into(),
        }
    }
}

impl<T: Into<SeismicTransactionRequest>> From<EthereumTxEnvelope<T>> for SeismicTransactionRequest {
    fn from(value: EthereumTxEnvelope<T>) -> Self {
        value.into()
    }
}

impl Into<SeismicTransactionRequest> for WithOtherFields<SeismicTransactionRequest> {
    fn into(self) -> SeismicTransactionRequest {
        self.inner.into()
    }
}

impl From<SeismicTransactionRequest> for WithOtherFields<SeismicTransactionRequest> {
    fn from(value: SeismicTransactionRequest) -> Self {
        WithOtherFields::new(value)
    }
}

impl TransactionBuilder4844 for SeismicTransactionRequest {
    fn blob_sidecar(&self) -> Option<&alloy_consensus::BlobTransactionSidecar> {
        self.inner.sidecar.as_ref()
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        self.inner.max_fee_per_blob_gas
    }

    fn set_blob_sidecar(&mut self, blob_sidecar: alloy_consensus::BlobTransactionSidecar) {
        self.inner.sidecar = Some(blob_sidecar);
    }

    fn set_max_fee_per_blob_gas(&mut self, max_fee_per_blob_gas: u128) {
        self.inner.max_fee_per_blob_gas = Some(max_fee_per_blob_gas);
    }

    fn with_blob_sidecar(mut self, sidecar: alloy_consensus::BlobTransactionSidecar) -> Self {
        self.inner.sidecar = Some(sidecar);
        self
    }

    fn with_max_fee_per_blob_gas(mut self, max_fee_per_blob_gas: u128) -> Self {
        self.inner.max_fee_per_blob_gas = Some(max_fee_per_blob_gas);
        self
    }
}

impl TransactionBuilder7702 for SeismicTransactionRequest {
    fn authorization_list(&self) -> Option<&Vec<SignedAuthorization>> {
        self.inner.authorization_list()
    }

    fn set_authorization_list(&mut self, authorization_list: Vec<SignedAuthorization>) {
        self.inner.set_authorization_list(authorization_list);
    }
}

impl Decodable712 for SeismicTransactionRequest {
    fn decode_712(typed_data: &TypedDataRequest) -> Eip712Result<Self> {
        let tx = TxSeismic::eip712_decode(&typed_data.data)?;
        let signed_tx = tx.into_signed(typed_data.signature);

        // Note: into will not recover the signer address unless the k256 feature is enabled
        Ok(signed_tx.into())
    }
}

impl InputDecryptionElements for SeismicTransactionRequest {
    fn get_decryption_elements(&self) -> Result<TxSeismicElements, InputDecryptionElementsError> {
        self.seismic_elements.ok_or(InputDecryptionElementsError::NoElements)
    }

    fn get_input(&self) -> alloy_primitives::Bytes {
        self.inner.input.clone().into_input().unwrap()
    }

    fn set_input(
        &mut self,
        data: alloy_primitives::Bytes,
    ) -> Result<(), seismic_alloy_consensus::InputDecryptionElementsError> {
        let new_self = core::mem::take(self).input(data.into());
        *self = new_self;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::Bytes;

    use super::*;

    #[test]
    fn test_set_input_for_request() {
        let mut req = SeismicTransactionRequest::from_transaction(TxEip1559::default());
        let start_input = req.get_input();
        let data = Bytes::from("test");
        assert_ne!(data, start_input);

        req.set_input(data.clone()).unwrap();
        let end_input = req.get_input();
        assert_eq!(data, end_input);
    }
}
