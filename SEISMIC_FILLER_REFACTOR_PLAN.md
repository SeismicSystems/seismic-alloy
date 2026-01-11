# Seismic Transaction Filler Refactor Plan

## Current State Problems

### 1. **Hacky Filling Logic in Provider Code**
Currently, `send_transaction_internal` (provider.rs:52-84) and `seismic_call` (provider.rs:96-147) contain manual encryption logic:
- Getting TEE pubkey
- Creating encryption keypair
- Generating seismic elements
- Encrypting calldata
- Setting encrypted input

**This is wrong** - all this logic should be in dedicated fillers that run as part of the filler pipeline.

### 2. **Ugly API for Marking Seismic Transactions**
Users currently have to do:
```rust
let mut tx = seismic_foundry_tx_builder()
    .with_input(encrypted_input)
    .with_kind(TxKind::Call(contract_address))
    .into();
tx.inner.transaction_type = Some(TxSeismic::TX_TYPE);  // UGLY!
tx.seismic_elements = Some(elements);
```

**This is bad** - users shouldn't have to manually set `inner.transaction_type` or `seismic_elements`.

### 3. **Inconsistent Seismic Transaction Detection**
There are multiple ways the code detects if a tx is seismic:
- Check if `seismic_elements` is present
- Check if `transaction_type == TxSeismic::TX_TYPE`
- Use `should_encrypt_input` helper

This logic is scattered and inconsistent.

## Goals for Refactor

1. **Move all filling logic to proper fillers**
2. **Provide ergonomic builder API** (e.g., `.with_seismic()` or `.as_seismic()`)
3. **Future-proof for new seismic metadata** (prepare for other branch with extra fields)
4. **Smart filler activation based on seismic markers**
5. **Keep all tests passing**

## Proposed Architecture

### Phase 1: Create Seismic-Specific Fillers

Create new fillers in `crates/network/src/fillers.rs`:

#### **A. SeismicElementsFiller**
- **Responsibility**: Generate and set seismic elements if not already present
- **Triggers when**: Transaction is marked as seismic AND seismic_elements is None
- **Actions**:
  - Generate encryption keypair
  - Generate encryption nonce
  - Create TxSeismicElements with these values
  - Set seismic_elements on the transaction request

#### **B. SeismicEncryptionFiller**
- **Responsibility**: Encrypt calldata using seismic elements
- **Triggers when**: Transaction has seismic_elements AND input is not already encrypted
- **Actions**:
  - Get TEE pubkey from provider
  - Extract plaintext input
  - Use seismic_elements to encrypt input
  - Set encrypted input back on transaction
  - **Note**: This filler needs provider access to call `get_tee_pubkey()`

#### **C. SeismicTypeFiller**
- **Responsibility**: Set the transaction type to seismic
- **Triggers when**: Transaction has seismic_elements set
- **Actions**:
  - Set `transaction_type = Some(TxSeismic::TX_TYPE)`
  - Ensures type matches the presence of seismic data

### Phase 2: Improve Builder API

Add methods to `SeismicTransactionRequest`:

```rust
impl SeismicTransactionRequest {
    /// Mark this transaction as a seismic transaction.
    /// This will trigger seismic fillers to generate elements and encrypt calldata.
    pub fn with_seismic(mut self) -> Self {
        // Set a marker that tells fillers to activate
        // Option 1: Set transaction_type
        self.inner.transaction_type = Some(TxSeismic::TX_TYPE);
        self
    }

    /// Mark this transaction as a seismic transaction (mutable version).
    pub fn set_seismic(&mut self) {
        self.inner.transaction_type = Some(TxSeismic::TX_TYPE);
    }

    /// Check if this transaction is marked as seismic
    pub fn is_seismic(&self) -> bool {
        self.inner.transaction_type == Some(TxSeismic::TX_TYPE)
            || self.seismic_elements.is_some()
    }
}
```

With this API, users can write:
```rust
let tx = seismic_foundry_tx_builder()
    .with_input(plaintext)  // plaintext!
    .with_kind(TxKind::Call(contract_address))
    .with_seismic()  // Clean API!
    .into();

provider.send_transaction(tx.into()).await
```

### Phase 3: Filler Logic & Ordering

The filler pipeline should look like:

```
1. RecommendedFillers (nonce, chain_id, etc.)
2. SeismicTypeFiller (if seismic_elements present, set type)
3. SeismicElementsFiller (if marked seismic & no elements, generate them)
4. SeismicGasFiller (already exists - handles gas for seismic txs)
5. SeismicEncryptionFiller (if has seismic_elements, encrypt input)
6. WalletFiller (sign the transaction)
```

**Key insight**: We need to determine "is this a seismic tx?" early in the pipeline.

#### Seismic Transaction Detection Rules:

A transaction is seismic if **any** of these are true:
1. `transaction_type == Some(TxSeismic::TX_TYPE)`
2. `seismic_elements.is_some()`

If user sets elements manually, we assume they've already encrypted calldata.

```rust
// Decision tree:
if is_marked_seismic() {
    if seismic_elements.is_none() {
        // User wants seismic but hasn't set elements
        // → SeismicElementsFiller generates elements
        // → SeismicEncryptionFiller encrypts the input
    } else {
        // User provided elements
        // → Assume input is already encrypted
        // → Skip encryption
    }
} else if seismic_elements.is_some() {
    // Elements present but not marked as seismic type
    // → Treat as seismic (set type)
    // → Assume input already encrypted
}
```

### Phase 4: Clean Up Provider Code

**Remove** all encryption logic from:
- `send_transaction_internal` (provider.rs:52-84)
- `seismic_call` (provider.rs:96-147)

These methods should just delegate to the filler pipeline and let fillers handle everything.

**Exception**: `seismic_call` is special because it:
1. Encrypts input before the call
2. Decrypts output after the call

For this, we might keep custom logic in `seismic_call`, OR we could:
- Have a special `SeismicCallFiller` that runs only for calls
- Keep the decryption logic in the provider since it's response-side

### Phase 5: Future-Proofing for New Seismic Fields

The other branch adds extra fields to seismic transactions and changes encryption flow (need all metadata before encrypting).

**Design decisions**:

1. **Seismic elements must be generated before gas estimation**
   - Current: SeismicElementsFiller runs after GasFiller
   - Future: May need to move SeismicElementsFiller earlier if encryption affects gas

2. **Encryption needs all transaction metadata**
   - Current: We can encrypt immediately after generating elements
   - Future: We might need to delay encryption until ALL fields are filled
   - Solution: Make SeismicEncryptionFiller run LATE in the pipeline (after all metadata fillers)

3. **New seismic fields**
   - Add new fillers for new fields (e.g., `SeismicMetadataFiller`)
   - Keep each filler focused on one concern

**Recommended filler order for future**:
```
1. RecommendedFillers (nonce, chain_id, etc.)
2. SeismicTypeFiller
3. SeismicElementsFiller (generate keypair/nonce)
4. SeismicMetadataFiller (NEW - future extra fields)
5. SeismicGasFiller
6. SeismicEncryptionFiller (must run AFTER all metadata is set)
7. WalletFiller
```

## Implementation Steps

### Step 1: Add builder methods
- Add `with_seismic()` / `set_seismic()` / `is_seismic()` to SeismicTransactionRequest
- Update tests to use new API

### Step 2: Create SeismicTypeFiller
- Simple filler that sets transaction_type based on seismic_elements presence
- Add to filler pipeline

### Step 3: Create SeismicElementsFiller
- Generate keypair and nonce
- Set seismic_elements if not present and tx is marked seismic
- Add to filler pipeline

### Step 4: Create SeismicEncryptionFiller
- Most complex - needs provider access for `get_tee_pubkey()`
- Encrypt input if seismic_elements present and input not empty
- Handle the case where user already encrypted (skip encryption)
- Add to filler pipeline

### Step 5: Update filler pipeline in provider constructors
- Modify `SeismicSignedProvider::new` and `SeismicUnsignedProvider::new_http`
- Add new fillers to the `JoinFill` chain

### Step 6: Simplify provider code
- Remove manual encryption from `send_transaction_internal`
- Decide on `seismic_call` - keep or refactor?

### Step 7: Update all tests
- Use new `.with_seismic()` API
- Remove manual setting of `transaction_type`
- Verify all tests still pass

## Open Questions

1. **How to handle SeismicEncryptionFiller's need for provider access?**
   - Fillers typically don't have provider access directly
   - Options:
     a) Pass provider to `prepare()` method (already available!)
     b) Store provider reference in filler (breaks current filler pattern)
     c) Use a different pattern for encryption (custom layer?)

2. **Should encryption happen in filler or in SeismicProvider layer?**
   - Current architecture: SeismicProvider wraps other providers
   - Could keep encryption in SeismicProvider::send_transaction_internal
   - But use fillers to PREPARE elements (not encrypt)
   - This maintains clearer separation

3. **How to detect "already encrypted" vs "needs encryption"?**
   - If user provides seismic_elements, assume encrypted
   - If filler generates seismic_elements, must encrypt
   - Need a marker to track this

4. **What about seismic_call?**
   - It's not a transaction send, it's a call
   - Encryption/decryption cycle is unique to calls
   - Probably should keep custom logic for this

## Recommended Approach (Hybrid)

After thinking through the constraints, here's the best approach:

### **Fillers Handle Element Generation**
- SeismicTypeFiller: Set transaction type
- SeismicElementsFiller: Generate keypair/nonce/elements

### **SeismicProvider Layer Handles Encryption**
Keep encryption in `send_transaction_internal`, but make it cleaner:

```rust
async fn send_transaction_internal(
    &self,
    mut tx: SendableTx<N>,
) -> TransportResult<PendingTransactionBuilder<N>> {
    // At this point, fillers have already run
    // - Type is set
    // - Elements are generated
    // - Gas is estimated

    if let Some(mut builder) = tx.as_mut_builder() {
        if self.should_encrypt_input(builder) {
            // Only encrypt if:
            // 1. It's a seismic tx (type is set)
            // 2. Elements are present
            // 3. Input is not empty
            // 4. Elements were generated by filler (not user-provided)

            let network_pk = self.get_tee_pubkey().await?;
            let seismic_elements = /* get from builder */;

            let plaintext_input = N::get_request_input(builder).unwrap();
            let encrypted_input = seismic_elements.client_encrypt(
                &plaintext_input,
                &network_pk,
                &encryption_keypair.secret_key()
            )?;

            N::set_request_input(builder, encrypted_input)?;
        }
    }
    self.inner.send_transaction_internal(tx).await
}
```

**But wait** - we have a problem: encryption keypair is generated in filler, but secret key is needed in provider.

### **Better Approach: Store Secret Key**

In `SeismicElementsFiller`:
1. Generate keypair
2. Store public key in seismic_elements
3. Store secret key... where?

Options:
- Add `encryption_secret_key: Option<SecretKey>` to SeismicTransactionRequest (temporary field)
- Use a thread-local or request-scoped storage
- Pass it through builder extensions

**Actually, let's look at the code again** - I see in provider.rs:64 that the keypair is generated inline. We need to preserve this keypair for encryption.

### **Simplest Working Solution**

Keep the encryption in `send_transaction_internal` as-is, but:
1. Add fillers to handle type detection and metadata
2. Use fillers to SET the seismic type marker
3. Let provider handle encryption (it already has all the context)

This means:
- **SeismicTypeFiller**: Set tx type if elements present
- **SeismicMetadataFiller**: For future - set extra metadata fields
- **Keep encryption in provider layer** (it's already there and works)

The main improvements:
1. Add `.with_seismic()` API to builder
2. Make type-setting automatic if elements present
3. Clean up the detection logic with consistent helpers
4. Prepare for future metadata fields with a metadata filler

This is less ambitious but more pragmatic and maintains the working encryption flow.

## Final Recommendation - REVISED

Based on user feedback, the ideal syntax should be:

```rust
let tx = seismic_foundry_tx_builder()
    .with_input(plaintext_bytes)
    .with_kind(TxKind::Call(contract_address))
    .seismic();  // Clean!
```

When the provider sees this, it should:
1. **First**: Fill in non-seismic elements (nonce, gas, chain_id)
2. **Then**: Generate seismic elements (encryption pubkey, encryption nonce, message_version=0)
3. **Finally**: Use those seismic elements to encrypt the calldata

### Implementation Plan

**IMPORTANT CONSTRAINTS:**
- For **rpc-types** and **network** crates:
  - ✅ Add new functions only
  - ❌ Do NOT modify existing functions
  - 📍 Place additions at the END of files/impl blocks to avoid merge conflicts
- For **provider** crate:
  - ✅ Can modify existing functions (this is the main refactor target)
  - ✅ Can replace/rewrite implementations as needed

---

**Step 1: Add `.seismic()` builder method with validation**

**File:** `crates/rpc-types/src/transaction/request.rs`
**Location:** Add a NEW impl block at the END of the file, after the existing impls

```rust
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

    /// Check if this transaction is marked as seismic
    pub fn is_seismic(&self) -> bool {
        self.inner.transaction_type == Some(TxSeismic::TX_TYPE)
            || self.seismic_elements.is_some()
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
                     Either call .seismic() or remove seismic_elements."
                );
            }
        }
        Ok(())
    }
}
```

**Step 2: Store encryption keypair in fillers**

**File:** `crates/network/src/fillers.rs`
**Location:** Add at the END of the file, after the existing `SeismicGasFiller` impl

Instead of generating a new keypair per transaction, generate ONE keypair per provider and store it in the fillers. This is simpler and more efficient.

```rust
// ============================================================================
// NEW: Seismic encryption keypair and additional fillers
// Added for filler-based seismic transaction handling
// ============================================================================

/// Shared encryption keypair used by seismic fillers.
/// One keypair per provider instance, reused for all transactions.
#[derive(Clone, Debug)]
pub struct SeismicEncryptionKeypair {
    keypair: seismic_enclave::secp256k1::Keypair,
}

impl SeismicEncryptionKeypair {
    /// Generate a new random keypair
    pub fn new() -> Self {
        Self {
            keypair: seismic_alloy_consensus::TxSeismicElements::get_rand_encryption_keypair(),
        }
    }

    /// Create from an existing secret key
    pub fn from_secret_key(secret_key: seismic_enclave::secp256k1::SecretKey) -> Self {
        let secp = seismic_enclave::secp256k1::Secp256k1::new();
        let keypair = seismic_enclave::secp256k1::Keypair::from_secret_key(&secp, &secret_key);
        Self { keypair }
    }

    /// Get the public key
    pub fn public_key(&self) -> seismic_enclave::secp256k1::PublicKey {
        self.keypair.public_key()
    }

    /// Get the secret key
    pub fn secret_key(&self) -> seismic_enclave::secp256k1::SecretKey {
        self.keypair.secret_key()
    }
}

impl Default for SeismicEncryptionKeypair {
    fn default() -> Self {
        Self::new()
    }
}
```

No need to modify `SeismicTransactionRequest` at all!

**Step 3: Create SeismicElementsFiller**

**File:** `crates/network/src/fillers.rs`
**Location:** Continue in the same section, right after `SeismicEncryptionKeypair` from Step 2

This filler stores the encryption keypair and uses it to generate seismic elements:

```rust
// Continue in the same "NEW" section...
/// Generates seismic elements for transactions marked as seismic.
/// Sets encryption pubkey, encryption nonce, and message_version.
/// Uses a stored keypair (one per provider instance).
#[derive(Clone)]
pub struct SeismicElementsFiller {
    encryption_keypair: SeismicEncryptionKeypair,
}

impl SeismicElementsFiller {
    pub fn new() -> Self {
        Self {
            encryption_keypair: SeismicEncryptionKeypair::new(),
        }
    }

    pub fn new_with_keypair(encryption_keypair: SeismicEncryptionKeypair) -> Self {
        Self { encryption_keypair }
    }

    pub fn encryption_keypair(&self) -> &SeismicEncryptionKeypair {
        &self.encryption_keypair
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicElementsFiller {
    type Fillable = ();

    fn status(&self, tx: &N::TransactionRequest) -> FillerControlFlow {
        // Validate consistency first
        if let Err(e) = tx.validate_seismic_consistency() {
            // Return error if inconsistent (will be caught during filling)
            return FillerControlFlow::Ready; // Will error in fill_sync
        }

        if tx.needs_seismic_elements() {
            FillerControlFlow::Ready
        } else {
            FillerControlFlow::Finished
        }
    }

    fn fill_sync(&self, tx: &mut SendableTx<N>) {
        if let Some(builder) = tx.as_mut_builder() {
            // Validate consistency - panic if invalid (should never happen if status was checked)
            builder.validate_seismic_consistency()
                .expect("Invalid seismic transaction configuration");

            if builder.needs_seismic_elements() {
                // Use stored keypair's public key
                let elements = TxSeismicElements::default()
                    .with_encryption_pubkey(self.encryption_keypair.public_key())
                    .with_encryption_nonce(TxSeismicElements::get_rand_encryption_nonce())
                    .with_message_version(0);

                // Store elements in the builder
                builder.set_seismic_elements(elements);
            }
        }
    }

    async fn prepare<P>(&self, _provider: &P, tx: &N::TransactionRequest)
        -> TransportResult<Self::Fillable>
    where P: Provider<N>
    {
        // Validate and return error early if inconsistent
        tx.validate_seismic_consistency()
            .map_err(|e| TransportErrorKind::custom_str(e))?;
        Ok(())
    }

    async fn fill(&self, _fillable: Self::Fillable, tx: SendableTx<N>)
        -> TransportResult<SendableTx<N>>
    {
        Ok(tx)
    }
}
```

**Step 4: Create SeismicEncryptionFiller**

**File:** `crates/network/src/fillers.rs`
**Location:** Continue in the same section, right after `SeismicElementsFiller` from Step 3

This filler also stores the encryption keypair and uses it to encrypt:

```rust
// Continue in the same "NEW" section...
/// Encrypts transaction input using seismic elements.
/// Uses a stored keypair (one per provider instance).
#[derive(Clone)]
pub struct SeismicEncryptionFiller {
    encryption_keypair: SeismicEncryptionKeypair,
}

impl SeismicEncryptionFiller {
    pub fn new(encryption_keypair: SeismicEncryptionKeypair) -> Self {
        Self { encryption_keypair }
    }
}

impl<N: SeismicNetwork> TxFiller<N> for SeismicEncryptionFiller {
    type Fillable = PublicKey; // TEE public key

    fn status(&self, tx: &N::TransactionRequest) -> FillerControlFlow {
        if tx.is_seismic() && tx.seismic_elements.is_some() {
            let input = N::get_request_input(tx);
            if input.map_or(false, |i| !i.is_empty()) {
                // Has seismic elements and non-empty input - needs encryption
                FillerControlFlow::Ready
            } else {
                // Empty input, nothing to encrypt
                FillerControlFlow::Finished
            }
        } else {
            FillerControlFlow::Finished
        }
    }

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {}

    async fn prepare<P>(&self, provider: &P, _tx: &N::TransactionRequest)
        -> TransportResult<Self::Fillable>
    where P: Provider<N>
    {
        // Get TEE pubkey from provider
        provider.get_tee_pubkey().await
    }

    async fn fill(&self, network_pk: Self::Fillable, mut tx: SendableTx<N>)
        -> TransportResult<SendableTx<N>>
    {
        if let Some(builder) = tx.as_mut_builder() {
            if let Some(elements) = builder.seismic_elements {
                // Encrypt the input using stored secret key
                let plaintext = N::get_request_input(builder).unwrap();
                if !plaintext.is_empty() {
                    let encrypted = elements
                        .client_encrypt(&plaintext, &network_pk, &self.encryption_keypair.secret_key())
                        .map_err(|e| TransportErrorKind::custom_str(
                            &format!("Error encrypting input: {:?}", e)
                        ))?;

                    N::set_request_input(builder, encrypted)
                        .map_err(|_| TransportErrorKind::custom_str("Error setting encrypted input"))?;
                }
            }
        }
        Ok(tx)
    }
}
```

**Step 5: Update filler pipeline and add new constructors**

**File:** `crates/provider/src/provider.rs`
**Location:**
- REPLACE the existing `SeismicSignedProvider::new()` implementation (lines ~194-208)
- ADD new `new_with_encryption_sk()` and `new_with_encryption_keypair()` methods
- Do the same for `SeismicUnsignedProvider`

Update `SeismicSignedProvider` with new constructors:

```rust
impl<N: SeismicNetwork> SeismicSignedProvider<N>
where
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Creates a new seismic signed provider with auto-generated encryption keypair
    pub fn new(wallet: impl Into<SeismicWallet<N>>, url: reqwest::Url) -> Self {
        Self::new_with_encryption_keypair(wallet, url, SeismicEncryptionKeypair::new())
    }

    /// Creates a new seismic signed provider with a specific encryption secret key
    pub fn new_with_encryption_sk(
        wallet: impl Into<SeismicWallet<N>>,
        url: reqwest::Url,
        secret_key: SecretKey,
    ) -> Self {
        Self::new_with_encryption_keypair(
            wallet,
            url,
            SeismicEncryptionKeypair::from_secret_key(secret_key),
        )
    }

    /// Internal constructor that creates provider with given encryption keypair
    fn new_with_encryption_keypair(
        wallet: impl Into<SeismicWallet<N>>,
        url: reqwest::Url,
        encryption_keypair: SeismicEncryptionKeypair,
    ) -> Self {
        // Create seismic fillers with shared encryption keypair
        let seismic_elements_filler = SeismicElementsFiller::new_with_keypair(encryption_keypair.clone());
        let seismic_encryption_filler = SeismicEncryptionFiller::new(encryption_keypair);

        // Build filler pipeline
        let tx_filler_layer = JoinFill::new(
            JoinFill::new(
                JoinFill::new(
                    JoinFill::new(
                        <N as RecommendedFillers>::recommended_fillers(),  // nonce, chain_id, etc.
                        seismic_elements_filler,  // generate seismic elements
                    ),
                    SeismicGasFiller::default(),  // handle gas for seismic txs
                ),
                seismic_encryption_filler,  // encrypt using seismic elements
            ),
            WalletFiller::new(wallet.into()),
        );

        // Build and return the provider
        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .connect_client(RpcClient::new_http(url));

        Self(inner)
    }
}
```

Similarly for `SeismicUnsignedProvider`:

```rust
impl<N: SeismicNetwork> SeismicUnsignedProvider<N>
where
    N::UnsignedTx: Send + Sync,
    RootProvider<N>: SeismicProviderExt<N>,
{
    /// Creates a new Seismic unsigned provider with an HTTP connection
    pub fn new_http(url: reqwest::Url) -> Self {
        Self::new_http_with_encryption_keypair(url, SeismicEncryptionKeypair::new())
    }

    /// Creates a new Seismic unsigned provider with specific encryption secret key
    pub fn new_http_with_encryption_sk(url: reqwest::Url, secret_key: SecretKey) -> Self {
        Self::new_http_with_encryption_keypair(
            url,
            SeismicEncryptionKeypair::from_secret_key(secret_key),
        )
    }

    /// Internal constructor
    fn new_http_with_encryption_keypair(
        url: reqwest::Url,
        encryption_keypair: SeismicEncryptionKeypair,
    ) -> Self {
        // Create seismic fillers with shared encryption keypair
        let seismic_elements_filler = SeismicElementsFiller::new_with_keypair(encryption_keypair.clone());
        let seismic_encryption_filler = SeismicEncryptionFiller::new(encryption_keypair);

        // Build filler pipeline
        let tx_filler_layer = JoinFill::new(
            JoinFill::new(
                JoinFill::new(
                    Identity,
                    <N as RecommendedFillers>::recommended_fillers(),
                ),
                seismic_elements_filler,
            ),
            JoinFill::new(
                SeismicGasFiller::default(),
                seismic_encryption_filler,
            ),
        );

        let inner = ProviderBuilder::<_, _, N>::default()
            .network::<N>()
            .layer(SeismicLayer {})
            .layer(tx_filler_layer)
            .connect_client(RpcClient::new_http(url));

        Self(inner)
    }

    // Similar for new_ws and new_ws_with_encryption_sk...
}
```

**Step 6: Remove encryption from provider layer**

**File:** `crates/provider/src/provider.rs`
**Location:** Replace the existing `send_transaction_internal` implementation (lines ~52-84)

Simplify `send_transaction_internal` by removing all the manual encryption logic:

```rust
async fn send_transaction_internal(
    &self,
    tx: SendableTx<N>,
) -> TransportResult<PendingTransactionBuilder<N>> {
    // Fillers have already:
    // 1. Generated seismic elements if needed
    // 2. Encrypted the input if needed
    // Just pass through to inner provider
    self.inner.send_transaction_internal(tx).await
}
```

**This removes ~30 lines of hacky encryption code!**

**Step 7: Handle seismic_call special case**

Keep custom logic in `seismic_call` since it needs to:
1. Force encryption even without `.seismic()` marker
2. Decrypt the response

```rust
async fn seismic_call(&self, mut tx: SendableTx<N>) -> TransportResult<Bytes> {
    // seismic_call always encrypts, even without .seismic() marker
    // This is a special case for calls (not transactions)
    // ... existing logic ...
}
```

### Filler Execution Order

1. **RecommendedFillers** - Fill nonce, chain_id, etc.
2. **SeismicElementsFiller** - Generate encryption pubkey, nonce, message_version
3. **SeismicGasFiller** - Estimate/set gas (seismic-aware)
4. **SeismicEncryptionFiller** - Encrypt input using seismic elements + TEE pubkey
5. **WalletFiller** - Sign the transaction

### Decision Logic Summary

```
# Validation first (in SeismicElementsFiller::prepare)
if tx.transaction_type is Some(non_seismic_type) AND tx.seismic_elements is Some:
    # ERROR: Inconsistent state!
    # e.g., transaction_type=TxType::Eip1559 but seismic_elements present
    → Return error: "Invalid transaction: non-seismic transaction type set with seismic elements"

# Normal flow
if tx.transaction_type == TxSeismic::TX_TYPE:
    # User called .seismic()
    if tx.seismic_elements is None:
        # → SeismicElementsFiller generates elements using stored keypair
        # → SeismicEncryptionFiller encrypts input using stored keypair
    else:
        # User provided elements manually (unusual case)
        # → Skip element generation
        # → SeismicEncryptionFiller still encrypts using stored keypair
        #   (assumes user wants encryption with their custom elements)

elif tx.seismic_elements is Some:
    # User provided elements without calling .seismic()
    # → Type not set, so not marked as seismic
    # → Seismic fillers skip this transaction
    # → Assume already encrypted
```

**Note**: The encryption keypair is now:
- Generated once per provider instance (or passed in via `new_with_encryption_sk`)
- Shared between `SeismicElementsFiller` and `SeismicEncryptionFiller`
- Reused for all transactions from that provider
- Never stored temporarily in the transaction request

### Benefits

✅ Clean API: `.seismic()`
✅ All filling logic in proper fillers
✅ Automatic element generation
✅ Automatic encryption
✅ Future-proof: easy to add more seismic fillers for new metadata
✅ Smart detection: user-provided elements skip encryption
✅ Separation of concerns: each filler does one thing

### Migration Path

Old code:
```rust
let mut tx = seismic_foundry_tx_builder()
    .with_input(encrypted_input)
    .with_kind(TxKind::Call(contract_address))
    .into();
tx.inner.transaction_type = Some(TxSeismic::TX_TYPE);
tx.seismic_elements = Some(elements);
```

New code:
```rust
let tx = seismic_foundry_tx_builder()
    .with_input(plaintext_input)  // plaintext now!
    .with_kind(TxKind::Call(contract_address))
    .seismic();  // that's it!
```

---

## Key Improvement: Stored Keypair Architecture

### Why This Is Better

**Before (per-transaction keypair):**
- Generate new keypair for every transaction
- Store secret key temporarily in transaction request
- Complex state management during filling
- More allocations and crypto operations

**After (provider-level keypair):**
- Generate ONE keypair per provider (or user-provided)
- Store in fillers, shared across all transactions
- No temporary state in transaction request
- Simpler, faster, and more secure

### API for Custom Encryption Keys

Users who want to control their encryption keys can use:

```rust
// Auto-generate keypair (default)
let provider = SeismicSignedProvider::new(wallet, url);

// Or provide your own secret key
let my_secret_key = SecretKey::from_slice(&[...])?;
let provider = SeismicSignedProvider::new_with_encryption_sk(wallet, url, my_secret_key);
```

Both providers share the same encryption key across all transactions they send.

### Why One Keypair Per Provider Is Safe

The encryption keypair is used for ECDH with the TEE's public key. Each transaction:
- Gets a unique encryption nonce (generated per transaction)
- Has unique plaintext
- Results in unique ciphertext

The nonce ensures that even with the same keypair, each encryption is unique and secure.

### Future Compatibility

When the other branch merges with new seismic metadata fields:
1. Add `SeismicMetadataFiller` to the pipeline
2. Runs after `SeismicElementsFiller`, before `SeismicEncryptionFiller`
3. Fills in new metadata fields
4. Encryption still happens last (after all metadata is ready)

The architecture naturally supports this without changes to the keypair management.

---

## Validation Examples

### ✅ Valid Configurations

```rust
// 1. Call .seismic() - generates elements automatically
let tx = seismic_foundry_tx_builder()
    .with_input(plaintext)
    .seismic();  // OK: type=seismic, elements=None → will be generated

// 2. Provide elements manually without .seismic()
let elements = TxSeismicElements::default()
    .with_encryption_pubkey(my_pubkey)
    .with_encryption_nonce(my_nonce);
let tx = seismic_foundry_tx_builder()
    .with_input(encrypted_input)
    .seismic_elements(elements);  // OK: type=None, elements=Some → assumes encrypted

// 3. Call .seismic() with custom elements (unusual but valid)
let tx = seismic_foundry_tx_builder()
    .with_input(plaintext)
    .seismic_elements(elements)
    .seismic();  // OK: type=seismic, elements=Some → will encrypt with provider keypair

// 4. Regular non-seismic transaction
let tx = seismic_foundry_tx_builder()
    .with_input(plaintext)
    .transaction_type(TxType::Eip1559);  // OK: type=eip1559, elements=None
```

### ❌ Invalid Configurations (Will Error)

```rust
// ERROR: Non-seismic type with seismic elements
let elements = TxSeismicElements::default()
    .with_encryption_pubkey(my_pubkey)
    .with_encryption_nonce(my_nonce);

let tx = seismic_foundry_tx_builder()
    .with_input(encrypted_input)
    .transaction_type(TxType::Eip1559)  // Type is EIP1559
    .seismic_elements(elements);  // But has seismic elements!

// When sent, SeismicElementsFiller::prepare() will return:
// TransportError: "Invalid transaction: non-seismic transaction type set with
//                  seismic elements. Either call .seismic() or remove seismic_elements."
```

### Why This Matters

Without validation, a user could accidentally:
1. Set type to EIP1559 (0x02)
2. Add seismic elements
3. Send transaction → network sees type 0x02, ignores seismic elements
4. Transaction is NOT encrypted or handled as seismic
5. Silent failure / security issue

With validation, we catch this early and give a clear error message.

---

## Implementation Summary

### Files to Modify

#### 1. `crates/rpc-types/src/transaction/request.rs` (ADD ONLY)
- **Line ~547 (end of file):** Add new impl block with helper methods
- Methods to add:
  - `seismic()` - mark transaction as seismic
  - `is_seismic()` - check if transaction is seismic
  - `needs_seismic_elements()` - check if elements need to be generated
  - `validate_seismic_consistency()` - validate type/elements consistency

#### 2. `crates/network/src/fillers.rs` (ADD ONLY)
- **Line ~112 (end of file):** Add new section with comment header
- Items to add:
  - `SeismicEncryptionKeypair` struct + impls
  - `SeismicElementsFiller` struct + TxFiller impl
  - `SeismicEncryptionFiller` struct + TxFiller impl

#### 3. `crates/provider/src/provider.rs` (MODIFY OK)
- **Lines ~52-84:** REPLACE `send_transaction_internal` - remove encryption logic
- **Lines ~194-208:** REPLACE `SeismicSignedProvider::new()` - add filler pipeline
- **Lines ~194-208:** ADD `new_with_encryption_sk()` method
- **Lines ~194-208:** ADD internal `new_with_encryption_keypair()` method
- **Lines ~226-261:** REPLACE `SeismicUnsignedProvider::new_http()` - add filler pipeline
- **Lines ~226-261:** ADD `new_http_with_encryption_sk()` method
- **Lines ~226-261:** ADD internal `new_http_with_encryption_keypair()` method
- **Lines ~248-261:** REPLACE `SeismicUnsignedProvider::new_ws()` - add filler pipeline (if implementing)
- **Lines ~96-147:** KEEP `seismic_call()` as-is (or minor modifications for consistency)

### Import Additions

#### In `crates/provider/src/provider.rs`
```rust
use seismic_alloy_network::fillers::{
    SeismicEncryptionKeypair, SeismicElementsFiller, SeismicEncryptionFiller
};
```

#### In `crates/network/src/fillers.rs`
```rust
// Already has most imports, may need:
use seismic_enclave::secp256k1::{Keypair, PublicKey, SecretKey, Secp256k1};
use seismic_alloy_consensus::TxSeismicElements;
```

### Testing Strategy

After implementation:
1. Run existing tests - they should all still pass
2. Update tests to use new `.seismic()` API
3. Tests that manually set `tx.inner.transaction_type = Some(TxSeismic::TX_TYPE)` should be updated to use `.seismic()`
4. Test the validation - add a test that tries to set non-seismic type + seismic elements (should error)

### Rollout Order

1. **First:** Add code to rpc-types (Step 1)
2. **Second:** Add code to network/fillers (Steps 2-4)
3. **Third:** Modify provider (Steps 5-6)
4. **Fourth:** Update tests
5. **Fifth:** Update examples/documentation
