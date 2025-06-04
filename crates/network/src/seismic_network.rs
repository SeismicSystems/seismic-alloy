use alloy_consensus::Transaction;
use alloy_network::{Network, TransactionBuilder};
use alloy_primitives::Bytes;
use seismic_alloy_consensus::{InputDecryptionElements, InputDecryptionElementsError, TxSeismicElements};
use crate::{foundry::SeismicFoundry, reth::Seismic};

/// A trait for networks that support seismic elements.
pub trait SeismicNetwork: Network {
    /// Set the seismic elements in the transaction request.
    fn set_seismic_elements(req: &mut Self::TransactionRequest, seismic_elements: TxSeismicElements);
    /// Get the request input from the transaction request.
    fn get_request_input(req: &Self::TransactionRequest) -> Option<&Bytes>;
    /// Get the envelope input from the transaction envelope.
    fn get_envelope_input(req: &Self::TxEnvelope) -> &Bytes;
    /// Set the input in the transaction envelope.
    fn set_input(req: &mut Self::TxEnvelope, input: Bytes) -> Result<(), InputDecryptionElementsError> ;
}

impl SeismicNetwork for Seismic {
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
}

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
}
