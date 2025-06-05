//! A trait for networks that support seismic elements.
use alloy_consensus::{EthereumTxEnvelope, Transaction};
use alloy_network::{Network, TransactionBuilder};
use alloy_primitives::{Address, Bytes};
use alloy_provider::fillers::RecommendedFillers;
use alloy_consensus::SignableTransaction;
use seismic_alloy_consensus::{InputDecryptionElements, InputDecryptionElementsError, TxSeismicElements};
use crate::{foundry::SeismicFoundry, reth::SeismicReth, wallet::SeismicWallet};

/// A trait for networks that support seismic elements.
#[async_trait::async_trait]
pub trait SeismicNetwork: Network + RecommendedFillers + Send + Sync {
    /// Set the seismic elements in the transaction request.
    fn set_seismic_elements(req: &mut Self::TransactionRequest, seismic_elements: TxSeismicElements);
    /// Get the request input from the transaction request.
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes>;
    /// Get the envelope input from the transaction envelope.
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes;
    /// Set the input in the transaction envelope.
    fn set_input(req: &mut Self::TxEnvelope, input: Bytes) -> Result<(), InputDecryptionElementsError> ;
    /// Sign a transaction from the given sender and transaction.
    async fn sign_transaction_from(wallet: &SeismicWallet<Self>, sender: Address, tx: Self::UnsignedTx) -> Result<Self::TxEnvelope, alloy_signer::Error>;
}

#[async_trait::async_trait]
impl SeismicNetwork for SeismicReth {
    fn set_seismic_elements(req: &mut Self::TransactionRequest, seismic_elements: TxSeismicElements) {
        req.set_seismic_elements(seismic_elements);
    }
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes> {
        req.inner.input.input()
    }
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes {
        req.input()
    }
    fn set_input(req: &mut Self::TxEnvelope, input: Bytes) -> Result<(), InputDecryptionElementsError> {
        req.set_input(input)
    }
    async fn sign_transaction_from(wallet: &SeismicWallet<Self>, sender: Address, tx: Self::UnsignedTx) -> Result<Self::TxEnvelope, alloy_signer::Error> {
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
}

#[async_trait::async_trait]
impl SeismicNetwork for SeismicFoundry {
    fn set_seismic_elements(req: &mut Self::TransactionRequest, seismic_elements: TxSeismicElements) {
        req.set_seismic_elements(seismic_elements);
    }
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes> {
        req.input()
    }
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes {
        req.input()
    }
    fn set_input(req: &mut Self::TxEnvelope, input: Bytes) -> Result<(), InputDecryptionElementsError> {
        req.set_input(input);
        Ok(())
    }
    async fn sign_transaction_from(wallet: &SeismicWallet<Self>, sender: Address, tx: Self::UnsignedTx) -> Result<Self::TxEnvelope, alloy_signer::Error> {
        match tx {
            Self::UnsignedTx::Ethereum(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                let signed = t.into_signed(sig);
                Ok(Self::TxEnvelope::Ethereum(EthereumTxEnvelope::from(signed)))
            }
            Self::UnsignedTx::Unknown(_) => {
                unimplemented!("Cannot sign unknown transaction type");
            }
            Self::UnsignedTx::Seismic(mut t) => {
                let sig = wallet.sign_transaction_inner(sender, &mut t).await?;
                Ok(t.into_signed(sig).into())
            }
        }
    }
}
