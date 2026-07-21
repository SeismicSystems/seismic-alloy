use alloc::vec::Vec;
use alloy_consensus::{
    transaction::{RlpEcdsaDecodableTx, RlpEcdsaEncodableTx},
    EthereumTxEnvelope, SignableTransaction, Signed, TxEip1559, TxEip2930, TxEip4844,
    TxEip4844Variant, TxEip7702, TxLegacy, TypedTransaction,
};
use alloy_eips::{eip7702::SignedAuthorization, Typed2718};
use alloy_network_primitives::{TransactionBuilder4844, TransactionBuilder7702};
use alloy_primitives::{Address, Bytes, Signature, TxKind, U256};
use alloy_rpc_types_eth::{AccessList, TransactionInput, TransactionRequest};
use alloy_serde::WithOtherFields;
use seismic_alloy_consensus::{
    Decodable712, Eip712Result, InputDecryptionElements, InputDecryptionElementsError,
    SeismicTxEnvelope, SeismicTxType, SeismicTypedTransaction, TxSeismic, TxSeismicElements,
    TxSeismicMetadata, TypedDataRequest, SEISMIC_TX_TYPE_ID,
};

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

    /// Seismic-specific elements (encryption metadata + freshness/replay fields).
    ///
    /// `Option` because this is a [`TransactionBuilder`] bag shared with non-Seismic requests:
    /// `None` for a plain transaction, `Some` for a Seismic tx. Note the `Option` tracks the
    /// *request* kind, not whether a `TxSeismic` had elements — `TxSeismic::seismic_elements` is
    /// mandatory, so any request built from a decoded Seismic tx (`From<TxSeismic>`, `decode_712`)
    /// always carries `Some`. Consumers that require elements (e.g. signed reads) must still check
    /// explicitly: nothing at the type level couples the request kind to this field being set.
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

    /// Initializes the [`SeismicTransactionRequest`] with the provided transaction.
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
            authorization_list: self.inner.authorization_list.unwrap_or_default(),
        })
    }

    /// Builds [`SeismicTypedTransaction`] from this builder. See
    /// [`TransactionRequest::build_typed_tx`] for more info.
    pub fn build_typed_tx(self) -> Result<SeismicTypedTransaction, Self> {
        if self.seismic_elements.is_some() {
            let fallback = self.clone();
            let tx = self.build_seismic().map_err(|e| {
                eprintln!("Failed to build seismic transaction: {e}");
                fallback
            })?;
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
            TypedTransaction::Eip4844(tx) => Ok(SeismicTypedTransaction::Eip4844(tx.into())),
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

    fn decrypt_to_tx_request(
        &self,
        secret_key: &seismic_crypto::secp256k1::SecretKey,
    ) -> Result<TransactionRequest, InputDecryptionElementsError> {
        if self.seismic_elements.is_some() {
            let sender = match self.from {
                Some(addr) => addr,
                None => {
                    return Err(InputDecryptionElementsError::MissingField("sender"));
                }
            };
            let tx_metadata = self.metadata(sender)?;
            return match self.inner.input.input() {
                Some(ciphertext) => {
                    let plaintext = tx_metadata.decrypt(secret_key, ciphertext).map_err(|e| {
                        InputDecryptionElementsError::DecryptionError(e.to_string())
                    })?;
                    Ok(self.inner.clone().input(alloy_primitives::Bytes::from(plaintext).into()))
                }
                None => Err(InputDecryptionElementsError::MissingField("input")),
            };
        }
        return Err(InputDecryptionElementsError::NoElements);
    }

    /// Decrypts the seismic elements and returns a [`TransactionRequest`].
    pub fn to_transaction_request(
        &self,
        secret_key: &seismic_crypto::secp256k1::SecretKey,
    ) -> Result<TransactionRequest, InputDecryptionElementsError> {
        match self.transaction_type {
            Some(SEISMIC_TX_TYPE_ID) => {
                // if there are no elements, throw an error
                let tx_req = self.decrypt_to_tx_request(secret_key);
                if tx_req.is_err() {
                    println!("tx type but no elements: {tx_req:?}");
                }
                tx_req
            }
            None => {
                match self.decrypt_to_tx_request(secret_key) {
                    // if there's no type, return the decrypted request
                    // if the decryption actually works
                    Ok(tx_req) => Ok(tx_req),
                    Err(InputDecryptionElementsError::NoElements) => {
                        // if there are no elements and no type,
                        // then return the original request,
                        // bc then we hit the default type
                        Ok(self.inner.clone())
                    }
                    // if there's no type but there are elements,
                    // and the decryption fails, return an error
                    Err(e) => {
                        println!("No elements & no tx type");
                        Err(e)
                    }
                }
            }
            _ => Ok(self.inner.clone()),
        }
    }

    /// Check this builder's preferred type, based on the fields that are set.
    pub fn preferred_type(&self) -> SeismicTxType {
        if let Some(ty) = self.inner.transaction_type {
            if ty == TxSeismic::TX_TYPE {
                return SeismicTxType::Seismic;
            }
        }
        if self.seismic_elements.is_some() {
            return SeismicTxType::Seismic;
        }
        self.inner.preferred_type().into()
    }

    /// Return the tx type this request can be built as. Computed by checking
    /// the preferred type, and then checking for completeness.
    pub fn buildable_type(&self) -> Option<SeismicTxType> {
        let pref = self.preferred_type();
        match pref {
            SeismicTxType::Seismic => self.complete_seismic().ok(),
            _ => {
                let buildable_type = self.inner.buildable_type();
                match buildable_type {
                    Some(tx_type) => return Some(tx_type.into()),
                    None => return None,
                }
            }
        }?;
        Some(pref)
    }

    /// Check if all necessary keys are present to build a transaction.
    ///
    /// # Returns
    ///
    /// - Ok(type) if all necessary keys are present to build the preferred type.
    /// - Err((type, missing)) if some keys are missing to build the preferred type.
    pub fn missing_keys(&self) -> Result<SeismicTxType, (SeismicTxType, Vec<&'static str>)> {
        let pref = self.preferred_type();
        if let Err(missing) = match pref {
            SeismicTxType::Seismic => self.complete_seismic(),
            _ => {
                let res = self.inner.missing_keys();
                match res {
                    Ok(tx_type) => return Ok(tx_type.into()),
                    Err((tx_type, missing)) => return Err((tx_type.into(), missing)),
                }
            }
        } {
            Err((pref, missing))
        } else {
            Ok(pref)
        }
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

impl From<TxEip4844> for SeismicTransactionRequest {
    fn from(tx: TxEip4844) -> Self {
        let inner = TransactionRequest::from_transaction(tx);
        Self { inner, seismic_elements: None }
    }
}

impl From<TxSeismic> for SeismicTransactionRequest {
    fn from(tx: TxSeismic) -> Self {
        let ty = tx.ty();
        let TxSeismic {
            chain_id,
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            input,
            seismic_elements,
            authorization_list,
        } = tx;

        let inner = TransactionRequest {
            to: Some(to.into()),
            gas_price: Some(gas_price),
            gas: Some(gas_limit),
            value: Some(value),
            input: input.into(),
            nonce: Some(nonce),
            chain_id: Some(chain_id),
            transaction_type: Some(ty),
            authorization_list: if authorization_list.is_empty() {
                None
            } else {
                Some(authorization_list)
            },
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

impl<Eip4844> From<SeismicTypedTransaction<Eip4844>> for SeismicTransactionRequest
where
    Eip4844: RlpEcdsaEncodableTx
        + RlpEcdsaDecodableTx
        + Clone
        + serde::de::DeserializeOwned
        + serde::Serialize
        + SignableTransaction<Signature>,
{
    fn from(tx: SeismicTypedTransaction<Eip4844>) -> Self {
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
                Self { inner: TransactionRequest::from_transaction(tx), seismic_elements: None }
            }
            SeismicTypedTransaction::Eip7702(tx) => {
                Self { inner: tx.into(), seismic_elements: None }
            }
            SeismicTypedTransaction::Seismic(tx) => tx.into(),
        }
    }
}

impl<Eip4844> From<SeismicTxEnvelope<Eip4844>> for SeismicTransactionRequest
where
    Eip4844: RlpEcdsaEncodableTx
        + RlpEcdsaDecodableTx
        + Clone
        + serde::de::DeserializeOwned
        + serde::Serialize
        + SignableTransaction<Signature>
        + Into<SeismicTransactionRequest>,
{
    fn from(value: SeismicTxEnvelope<Eip4844>) -> Self {
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

    fn get_input(&self) -> Result<Bytes, InputDecryptionElementsError> {
        match self.inner.input.clone().into_input() {
            Some(input) => Ok(input),
            None => Err(InputDecryptionElementsError::MissingField("input")),
        }
    }

    fn set_input(
        &mut self,
        data: Bytes,
    ) -> Result<(), seismic_alloy_consensus::InputDecryptionElementsError> {
        let new_self = core::mem::take(self).input(data.into());
        *self = new_self;
        Ok(())
    }

    fn metadata(&self, sender: Address) -> Result<TxSeismicMetadata, InputDecryptionElementsError> {
        Ok(TxSeismicMetadata {
            sender,
            legacy_fields: seismic_alloy_consensus::TxLegacyFields {
                chain_id: self
                    .chain_id
                    .ok_or(InputDecryptionElementsError::MissingField("chain_id"))?,
                nonce: self.nonce.ok_or(InputDecryptionElementsError::MissingField("nonce"))?,
                to: self.to.ok_or(InputDecryptionElementsError::MissingField("to"))?,
                value: self.value.unwrap_or_default(),
            },
            seismic_elements: self
                .seismic_elements
                .ok_or(InputDecryptionElementsError::NoElements)?,
        })
    }
}

// ============================================================================
// NEW: Seismic transaction builder helpers and validation
// Added for filler-based seismic transaction handling
// ============================================================================

impl SeismicTransactionRequest {
    /// Mark this transaction as a seismic transaction.
    /// Fillers will generate seismic elements and encrypt the input.
    pub fn seismic(mut self) -> Self {
        self.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        self
    }

    /// Mark this seismic transaction as a signed call (sets signed_read to true).
    /// This should be called for seismic calls (eth_call), not sends (eth_sendTransaction).
    /// Creates partial elements with signed_read=true; the filler will complete them.
    pub fn with_signed_read(mut self) -> Self {
        // Create default elements with signed_read=true
        // The filler will see encryption_nonce=0 and know to complete the encryption
        if self.seismic_elements.is_none() {
            self.seismic_elements = Some(TxSeismicElements::default());
        }
        if let Some(ref mut elements) = self.seismic_elements {
            elements.signed_read = true;
        }
        self
    }

    /// Check if this transaction is marked as seismic
    pub fn is_seismic(&self) -> bool {
        // First check if explicitly marked as seismic type
        if self.inner.transaction_type == Some(TxSeismic::TX_TYPE) {
            return true;
        }

        // Only infer from seismic_elements if transaction_type is None
        if self.inner.transaction_type.is_none() && self.seismic_elements.is_some() {
            return true;
        }

        false
    }

    /// Check if this transaction needs seismic elements to be filled
    pub fn needs_seismic_elements(&self) -> bool {
        self.is_seismic() && self.seismic_elements.is_none()
    }

    /// Validate that transaction type and seismic elements are compatible.
    /// Returns an error if non-seismic type is set with seismic elements.
    pub fn validate_seismic_consistency(&self) -> Result<(), &'static str> {
        if let Some(tx_type) = self.inner.transaction_type {
            if tx_type != TxSeismic::TX_TYPE && self.seismic_elements.is_some() {
                return Err(
                    "Invalid transaction: non-seismic transaction type set with seismic elements. \
                     Either call .seismic() or remove seismic_elements.",
                );
            }
        }
        Ok(())
    }
}

// ============================================================================
// AsRef/AsMut implementations for better trait bound compatibility
// ============================================================================

impl AsRef<SeismicTransactionRequest> for SeismicTransactionRequest {
    fn as_ref(&self) -> &SeismicTransactionRequest {
        self
    }
}

impl AsMut<SeismicTransactionRequest> for SeismicTransactionRequest {
    fn as_mut(&mut self) -> &mut SeismicTransactionRequest {
        self
    }
}

// Note: AsRef<SeismicTransactionRequest> for WithOtherFields<SeismicTransactionRequest>
// is automatically provided by alloy_serde's blanket impl:
// impl<T, U> AsRef<U> for WithOtherFields<T> where T: AsRef<U>

impl AsMut<SeismicTransactionRequest> for WithOtherFields<SeismicTransactionRequest> {
    fn as_mut(&mut self) -> &mut SeismicTransactionRequest {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_input_for_request() {
        let mut req = SeismicTransactionRequest::from_transaction(TxEip1559::default());
        let start_input = req.get_input().unwrap();
        let data = Bytes::from("test");
        assert_ne!(data, start_input);

        req.set_input(data.clone()).unwrap();
        let end_input = req.get_input().unwrap();
        assert_eq!(data, end_input);
    }

    /// Regression test: get_input() must return an error instead of panicking
    /// when called on a SeismicTransactionRequest with no input data.
    #[test]
    fn get_input_returns_error_when_input_missing() {
        let req = SeismicTransactionRequest::default()
            .from(Address::ZERO)
            .nonce(0)
            .to(Address::ZERO)
            .seismic_elements(TxSeismicElements::default())
            .seismic();

        let result = req.get_input();
        assert!(result.is_err(), "get_input should return Err when input is missing");
    }

    /// Regression test: to_transaction_request() must return an error instead of
    /// panicking when seismic elements are present but calldata is missing.
    #[test]
    fn to_transaction_request_returns_error_when_input_missing() {
        let mut req = SeismicTransactionRequest::default()
            .from(Address::ZERO)
            .nonce(0)
            .to(Address::ZERO)
            .seismic_elements(TxSeismicElements::default())
            .seismic();
        req.inner.chain_id = Some(1);

        let sk = seismic_crypto::get_unsecure_sample_secp256k1_sk();
        let result = req.to_transaction_request(&sk);
        assert!(result.is_err(), "to_transaction_request should return Err when input is missing");
    }
}
