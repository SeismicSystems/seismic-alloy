//! A seismic provider trait that extends alloy_provider::Provider and is implimented for relevant
//! types. Extends the provider trait with ...
use alloy_network::{eip2718::Encodable2718, TransactionBuilder};
use alloy_primitives::Bytes;
use alloy_provider::{Provider, ProviderCall, SendableTx};
use alloy_rpc_client::NoParams;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::TxSeismicElements;
use seismic_alloy_network::seismic_network::SeismicNetwork;
use seismic_enclave::PublicKey;
use std::str::FromStr;
use tracing::warn;

/// Extends the alloy_provider::Provider with Seismic specific functionality
#[async_trait::async_trait]
pub trait SeismicProviderExt<N: SeismicNetwork>: Provider<N>
where
    N::UnsignedTx: Send + Sync,
{
    /// Makes a call request while handling seismic specific aspects
    /// e.g. encrypting input data and decrypting output data
    /// e.g. sending signed call requests
    async fn seismic_call(&self, mut tx: SendableTx<N>) -> TransportResult<Bytes> {
        println!("seismic_call entered. tx: {:?}\n", tx);
        if let Some(builder) = tx.as_mut_builder() {
            if self.should_encrypt_input(builder) {
                return self.call_with_encryption(tx).await;
            }
        }

        // If we get here, we are not encrypting the input data
        self.call_conditionally_signed(tx).await
    }

    /// Whether the input data should be encrypted
    /// None or Empty input data should not be encrypted
    fn should_encrypt_input<B: TransactionBuilder<N>>(&self, tx: &B) -> bool {
        tx.input().map_or(false, |input| !input.is_empty())
    }

    /// Get the PublicKey of the enclave
    async fn get_tee_pubkey(&self) -> TransportResult<PublicKey> {
        let call: ProviderCall<NoParams, String> =
            self.client().request_noparams("seismic_getTeePublicKey").into();
        let resp = call.await?;
        let stripped = resp.strip_prefix("0x").unwrap_or(&resp);
        match PublicKey::from_str(stripped) {
            Ok(pk) => Ok(pk),
            Err(e) => Err(TransportErrorKind::custom_str(&format!(
                "Error getting tee pubkey from server: {:?}",
                e
            ))),
        }
    }

    /// Encrypts the input data, runs self.call_conditionally_signed, and decrypts the output data
    async fn call_with_encryption(&self, mut tx: SendableTx<N>) -> TransportResult<Bytes> {
        // set up elements unrelated to the input tx
        let network_pk = self.get_tee_pubkey().await.map_err(|e| {
            TransportErrorKind::custom_str(&format!(
                "Error getting tee pubkey from server: {:?}",
                e
            ))
        })?;
        let encryption_keypair = TxSeismicElements::get_rand_encryption_keypair();
        let seismic_elements = TxSeismicElements::default()
            .with_encryption_pubkey(encryption_keypair.public_key())
            .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce());

        // Encrypt using recipient's public key and generated private key
        tx = match tx {
            SendableTx::Builder(mut builder) => {
                let plaintext_input = N::get_request_input(&builder).unwrap();
                let encrypted_input = seismic_elements
                    .client_encrypt(&plaintext_input, &network_pk, &encryption_keypair.secret_key())
                    .map_err(|e| {
                        TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e))
                    })?;

                TransactionBuilder::<N>::set_input(&mut builder, Bytes::from(encrypted_input));
                N::set_seismic_elements(&mut builder, seismic_elements);
                SendableTx::Builder(builder)
            }
            SendableTx::Envelope(mut envelope) => {
                let plaintext_input = N::get_envelope_input(&envelope);
                let encrypted_input = seismic_elements
                    .client_encrypt(&plaintext_input, &network_pk, &encryption_keypair.secret_key())
                    .map_err(|e| {
                        TransportErrorKind::custom_str(&format!("Error encrypting input: {:?}", e))
                    })?;
                N::set_input(&mut envelope, Bytes::from(encrypted_input)).map_err(|e| {
                    TransportErrorKind::custom_str(&format!(
                        "Error setting encrypted input: {:?}",
                        e
                    ))
                })?;
                SendableTx::Envelope(envelope)
            }
        };

        // make the rpc call
        println!("call_with_encryption. about to make inner.call, tx: {:?}\n", tx);
        let encrypted_output = self.call_conditionally_signed(tx).await?;

        // decrypt the output
        let decrypted_output = seismic_elements
            .client_decrypt(&encrypted_output, &network_pk, &encryption_keypair.secret_key())
            .map_err(|e| {
                TransportErrorKind::custom_str(&format!("Error decrypting output: {:?}", e))
            })?;

        return Ok(Bytes::from(decrypted_output));
    }

    /// Makes a call request, perhaps making the call signed depinding on the input type
    async fn call_conditionally_signed(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        println!("call_conditionally_signed entered. tx: {:?}\n", tx);
        match tx {
            SendableTx::Builder(builder) => {
                warn!("seismic_call: sending unsigned transaction");
                println!("seismic_call: sending unsigned transaction");
                let output = self.client().request("eth_call", (builder.clone(),)).await?;
                Ok(output)
            }
            SendableTx::Envelope(envelope) => {
                warn!("seismic_call: sending signed transaction");
                println!("seismic_call: sending signed transaction");

                let encoded_tx = envelope.encoded_2718();
                let output = self.client().request("eth_call", (encoded_tx,)).await?;
                Ok(output)
            }
        }
    }
}
