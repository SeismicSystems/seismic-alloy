//! A seismic provider trait that extends alloy_provider::Provider and is implimented for relevant
//! types. Extends the provider trait with ...
use alloy_network::{eip2718::Encodable2718, TransactionBuilder};
use alloy_primitives::Bytes;
use alloy_provider::{
    fillers::{FillProvider, TxFiller},
    Provider, ProviderCall, RootProvider, SendableTx,
};
use alloy_rpc_client::NoParams;
use alloy_transport::{TransportErrorKind, TransportResult};
use seismic_alloy_consensus::TxSeismicElements;
use seismic_alloy_network::{
    foundry::SeismicFoundry, seismic_network::SeismicNetwork, SeismicReth,
};
use seismic_enclave::PublicKey;
use std::str::FromStr;

/// Extends the alloy_provider::Provider with Seismic specific functionality
#[async_trait::async_trait]
pub trait SeismicProviderExt<N: SeismicNetwork>: Provider<N>
where
    N::UnsignedTx: Send + Sync,
{
    /// Makes a call request while handling seismic specific aspects
    /// e.g. encrypting input data and decrypting output data
    /// e.g. sending signed call requests
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        self.call_conditionally_signed(tx).await
    }

    /// Whether the input data should be encrypted
    /// If it's not a seismic tx, don't encrypt.
    /// If it is, only encrypt if it's non-empty
    fn should_encrypt_input<B: TransactionBuilder<N>>(&self, tx: &B) -> bool {
        if !N::is_seismic_tx_type(tx.output_tx_type()) {
            return false;
        }
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

    /// Makes a call request, perhaps making the call signed depinding on the input type
    async fn call_conditionally_signed(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        match tx {
            SendableTx::Builder(builder) => {
                let call_input = builder.clone();
                println!("call_input: {:?}", call_input);
                let output = self.client().request("eth_call", (call_input,)).await?;
                Ok(output)
            }
            SendableTx::Envelope(envelope) => {
                let encoded_tx = envelope.encoded_2718();
                println!("encoded_tx: {:?}", encoded_tx);
                let output = self.client().request("eth_call", (encoded_tx,)).await?;
                Ok(output)
            }
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
                N::set_envelope_input(&mut envelope, Bytes::from(encrypted_input)).map_err(
                    |e| {
                        TransportErrorKind::custom_str(&format!(
                            "Error setting encrypted input: {:?}",
                            e
                        ))
                    },
                )?;
                SendableTx::Envelope(envelope)
            }
        };

        // make the rpc call
        let encrypted_output = self.call_conditionally_signed(tx).await?;

        // decrypt the output
        let decrypted_output = seismic_elements
            .client_decrypt(&encrypted_output, &network_pk, &encryption_keypair.secret_key())
            .map_err(|e| {
                TransportErrorKind::custom_str(&format!("Error decrypting output: {:?}", e))
            })?;

        return Ok(Bytes::from(decrypted_output));
    }
}

#[async_trait::async_trait]
impl SeismicProviderExt<SeismicReth> for RootProvider<SeismicReth> {
    async fn seismic_call(&self, tx: SendableTx<SeismicReth>) -> TransportResult<Bytes> {
        self.call_conditionally_signed(tx).await
    }
}
#[async_trait::async_trait]
impl SeismicProviderExt<SeismicFoundry> for RootProvider<SeismicFoundry> {
    async fn seismic_call(&self, tx: SendableTx<SeismicFoundry>) -> TransportResult<Bytes> {
        self.call_conditionally_signed(tx).await
    }
}

#[async_trait::async_trait]
impl<F: TxFiller<N>, P: Provider<N>, N: SeismicNetwork> SeismicProviderExt<N>
    for FillProvider<F, P, N>
where
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    async fn seismic_call(&self, tx: SendableTx<N>) -> TransportResult<Bytes> {
        // Fill the transaction
        let builder = tx.as_builder().unwrap().clone();
        let built_tx = self.fill(builder).await?;

        // self.inner is not public for FillProvider.
        // However, for our use cases, self.inner is the RootProvider,
        // so we get it this hacky way
        let inner = self.root();
        SeismicProviderExt::seismic_call(inner, built_tx).await
    }
}
