//! A trait for networks that support seismic elements.
use crate::{foundry::SeismicFoundry, reth::SeismicReth, wallet::SeismicWallet};
use alloy_consensus::{EthereumTxEnvelope, SignableTransaction, Transaction};
use alloy_network::{Network, TransactionBuilder};
use alloy_primitives::{Address, Bytes};
use alloy_provider::fillers::RecommendedFillers;
use alloy_rpc_types_eth::TransactionInput;
use seismic_alloy_consensus::{
    InputDecryptionElements, InputDecryptionElementsError, TxSeismicElements, SEISMIC_TX_TYPE_ID,
};

/// A trait for networks that support seismic elements.
#[async_trait::async_trait]
pub trait SeismicNetwork: Network + RecommendedFillers + Send + Sync
where
    Self::UnsignedTx: Send + Sync,
{
    /// Set the seismic elements in the transaction request.
    fn set_seismic_elements(
        req: &mut Self::TransactionRequest,
        seismic_elements: TxSeismicElements,
    );
    /// Get the seismic elements from the transaction request.
    fn get_seismic_elements(req: &Self::TransactionRequest) -> Option<TxSeismicElements>;
    /// Get the request input from the transaction request.
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes>;
    /// Get the envelope input from the transaction envelope.
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes;
    /// Set the input in the transaction envelope.
    fn set_request_input(
        req: &mut Self::TransactionRequest,
        input: Bytes,
    ) -> Result<(), InputDecryptionElementsError>;
    /// Set the input in the transaction request.
    fn set_envelope_input(
        req: &mut Self::TxEnvelope,
        input: Bytes,
    ) -> Result<(), InputDecryptionElementsError>;
    /// Sign a transaction from the given sender and transaction.
    async fn sign_transaction_from(
        wallet: &SeismicWallet<Self>,
        sender: Address,
        tx: Self::UnsignedTx,
    ) -> Result<Self::TxEnvelope, alloy_signer::Error>;
    /// True if the transaction type is a seismic transaction.
    fn is_seismic_tx_type(ty: Self::TxType) -> bool;

    /// Extract metadata from a seismic envelope for decryption.
    /// Returns None if the envelope is not a seismic transaction.
    fn extract_seismic_metadata(
        envelope: &Self::TxEnvelope,
    ) -> Option<seismic_alloy_consensus::TxSeismicMetadata>;
}

#[async_trait::async_trait]
impl SeismicNetwork for SeismicReth {
    fn set_seismic_elements(
        req: &mut Self::TransactionRequest,
        seismic_elements: TxSeismicElements,
    ) {
        req.set_seismic_elements(seismic_elements);
    }
    fn get_seismic_elements(req: &Self::TransactionRequest) -> Option<TxSeismicElements> {
        req.seismic_elements
    }
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes> {
        req.inner.input.input()
    }
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes {
        req.input()
    }
    fn set_request_input(
        req: &mut Self::TransactionRequest,
        input: Bytes,
    ) -> Result<(), InputDecryptionElementsError> {
        req.inner.input = TransactionInput::new(input);
        Ok(())
    }
    fn set_envelope_input(
        req: &mut Self::TxEnvelope,
        input: Bytes,
    ) -> Result<(), InputDecryptionElementsError> {
        req.set_input(input)
    }
    async fn sign_transaction_from(
        wallet: &SeismicWallet<Self>,
        sender: Address,
        tx: Self::UnsignedTx,
    ) -> Result<Self::TxEnvelope, alloy_signer::Error> {
        match tx {
            Self::UnsignedTx::Legacy(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
            Self::UnsignedTx::Eip2930(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
            Self::UnsignedTx::Eip1559(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
            Self::UnsignedTx::Eip4844(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
            Self::UnsignedTx::Eip7702(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
            Self::UnsignedTx::Seismic(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
        }
    }

    fn is_seismic_tx_type(ty: Self::TxType) -> bool {
        match ty {
            Self::TxType::Seismic => true,
            _ => false,
        }
    }

    fn extract_seismic_metadata(
        envelope: &Self::TxEnvelope,
    ) -> Option<seismic_alloy_consensus::TxSeismicMetadata> {
        use alloy_consensus::transaction::SignableTransaction;
        use alloy_primitives::Address;
        use seismic_alloy_consensus::{SeismicTxEnvelope, TxLegacyFields, TxSeismicMetadata};

        match envelope {
            SeismicTxEnvelope::Seismic(signed_tx) => {
                let tx_seismic = signed_tx.tx();
                let signature = signed_tx.signature();

                // Recover sender from signature
                let signature_hash = tx_seismic.signature_hash();
                let recovered_pubkey = signature.recover_from_prehash(&signature_hash).ok()?;
                let sender = Address::from_public_key(&recovered_pubkey);

                Some(TxSeismicMetadata {
                    sender,
                    legacy_fields: TxLegacyFields {
                        chain_id: tx_seismic.chain_id,
                        nonce: tx_seismic.nonce,
                        to: tx_seismic.to,
                        value: tx_seismic.value,
                    },
                    seismic_elements: tx_seismic.seismic_elements.clone(),
                })
            }
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl SeismicNetwork for SeismicFoundry {
    fn set_seismic_elements(
        req: &mut Self::TransactionRequest,
        seismic_elements: TxSeismicElements,
    ) {
        req.set_seismic_elements(seismic_elements);
    }
    fn get_seismic_elements(req: &Self::TransactionRequest) -> Option<TxSeismicElements> {
        req.seismic_elements
    }
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes> {
        req.input()
    }
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes {
        req.input()
    }
    fn set_request_input(
        req: &mut Self::TransactionRequest,
        input: Bytes,
    ) -> Result<(), InputDecryptionElementsError> {
        InputDecryptionElements::set_input(req, input)
    }
    fn set_envelope_input(
        req: &mut Self::TxEnvelope,
        input: Bytes,
    ) -> Result<(), InputDecryptionElementsError> {
        InputDecryptionElements::set_input(req, input)
    }
    async fn sign_transaction_from(
        wallet: &SeismicWallet<Self>,
        sender: Address,
        tx: Self::UnsignedTx,
    ) -> Result<Self::TxEnvelope, alloy_signer::Error> {
        match tx {
            Self::UnsignedTx::Ethereum(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                let signed = t.into_signed(sig);
                Ok(Self::TxEnvelope::Ethereum(EthereumTxEnvelope::from(signed)))
            }
            Self::UnsignedTx::Unknown(_) => {
                return Err(alloy_signer::Error::other("Cannot sign unknown transaction type"));
            }
            Self::UnsignedTx::Seismic(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
        }
    }

    fn is_seismic_tx_type(ty: Self::TxType) -> bool {
        match ty.0 {
            SEISMIC_TX_TYPE_ID => true,
            _ => false,
        }
    }

    fn extract_seismic_metadata(
        envelope: &Self::TxEnvelope,
    ) -> Option<seismic_alloy_consensus::TxSeismicMetadata> {
        use crate::foundry::envelope::SeismicFoundryTxEnvelope;
        use alloy_consensus::transaction::SignableTransaction;
        use alloy_primitives::Address;
        use seismic_alloy_consensus::{TxLegacyFields, TxSeismicMetadata};

        match envelope {
            SeismicFoundryTxEnvelope::Seismic(signed_tx) => {
                let tx_seismic = signed_tx.tx();
                let signature = signed_tx.signature();

                // Recover sender from signature
                let signature_hash = tx_seismic.signature_hash();
                let recovered_pubkey = signature.recover_from_prehash(&signature_hash).ok()?;
                let sender = Address::from_public_key(&recovered_pubkey);

                Some(TxSeismicMetadata {
                    sender,
                    legacy_fields: TxLegacyFields {
                        chain_id: tx_seismic.chain_id,
                        nonce: tx_seismic.nonce,
                        to: tx_seismic.to,
                        value: tx_seismic.value,
                    },
                    seismic_elements: tx_seismic.seismic_elements.clone(),
                })
            }
            _ => None,
        }
    }
}
