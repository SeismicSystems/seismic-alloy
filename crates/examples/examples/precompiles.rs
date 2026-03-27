//! Calling Seismic's custom precompiles from client code.
//!
//! Demonstrates using the precompile helpers to call Seismic's on-chain
//! cryptographic primitives: RNG, ECDH, AES-GCM, HKDF, and SECP256K1 signing.
//!
//! # Running
//!
//! Requires `sanvil` in `$PATH` or `$HOME/.seismic/bin/`.
//!
//! ```sh
//! cargo run -p seismic-examples --example precompiles
//! ```

use alloy_node_bindings::Anvil;
use seismic_alloy_network::foundry::SeismicFoundry;
use seismic_alloy_provider::precompiles;
use seismic_prelude::client::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let anvil = Anvil::at("sanvil").spawn();
    let signer: PrivateKeySigner = anvil.keys()[1].clone().into();
    let wallet = SeismicWallet::<SeismicFoundry>::from(signer);

    let provider = SeismicProviderBuilder::new()
        .foundry()
        .wallet(wallet)
        .connect_http(anvil.endpoint_url())
        .await?;

    println!("Connected to sanvil at {}", anvil.endpoint());
    println!("Precompile addresses:");
    println!("  RNG:           {}", precompiles::addresses::RNG);
    println!("  ECDH:          {}", precompiles::addresses::ECDH);
    println!("  AES Encrypt:   {}", precompiles::addresses::AES_ENCRYPT);
    println!("  AES Decrypt:   {}", precompiles::addresses::AES_DECRYPT);
    println!("  HKDF:          {}", precompiles::addresses::HKDF);
    println!("  SECP256K1:     {}", precompiles::addresses::SECP256K1_SIGN);

    // ---- RNG: generate 32 random bytes ----
    let random_bytes =
        precompiles::call::rng::<SeismicFoundry, _>(&provider, 32, b"example_domain").await?;
    println!("\nRNG (32 bytes): {random_bytes}");
    assert_eq!(random_bytes.len(), 32);

    // ---- HKDF: derive a key from input material ----
    let derived_key =
        precompiles::call::hkdf::<SeismicFoundry, _>(&provider, b"my secret input").await?;
    println!("HKDF derived key: {derived_key}");

    // ---- AES-GCM: encrypt then decrypt ----
    let aes_key = derived_key; // use the HKDF-derived key
    let nonce = FixedBytes::<12>::from([1u8; 12]);
    let plaintext = b"Hello, Seismic!";

    let ciphertext =
        precompiles::call::aes_encrypt::<SeismicFoundry, _>(&provider, &aes_key, &nonce, plaintext)
            .await?;
    println!("AES encrypted: {ciphertext} ({} bytes)", ciphertext.len());
    assert_eq!(ciphertext.len(), plaintext.len() + 16); // plaintext + 16-byte GCM tag

    let decrypted = precompiles::call::aes_decrypt::<SeismicFoundry, _>(
        &provider,
        &aes_key,
        &nonce,
        &ciphertext,
    )
    .await?;
    println!("AES decrypted: {:?}", String::from_utf8_lossy(&decrypted));
    assert_eq!(decrypted.as_ref(), plaintext);

    println!("\nAll precompile calls completed successfully!");
    Ok(())
}
