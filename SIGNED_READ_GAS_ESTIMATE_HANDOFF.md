# Signed-read gas estimate — reviewer handoff

Reviewer handoff for [seismic-alloy #119](https://github.com/SeismicSystems/seismic-alloy/pull/119).
Branch: `hbai__signed-read-gas-estimate`.

## What broke

Signed gas estimation sent a clone of the write to `eth_estimateGas` without setting
`signed_read = true`. Both nodes reject that, so any signed send without an explicit gas
limit fails.

| Node | Rejection | Enforced in |
| --- | --- | --- |
| sanvil | `signed call must set signed_read=true` | `crates/anvil/src/eth/api.rs`, `backend/mem/mod.rs` |
| reth | `signed simulation request must set signedRead=true` | `ensure_signed_read_request`, `crates/seismic/rpc/src/eth/utils.rs` |

This is not CI-only. A user calling `.seismic().send()` against reth without passing a gas
limit hits it too.

## Where it came from

Nobody wrote a bug. Two correct changes, five months apart, that nobody ran against each other.

| Date | Change | What it did |
| --- | --- | --- |
| 2026-04-08 | [seismic-alloy #98](https://github.com/SeismicSystems/seismic-alloy/pull/98) | Made gas estimation sign a twin of the write so the node could authenticate the sender. Did not set `signed_read` — nothing required it then. |
| 2026-09-16 | [seismic-foundry #220](https://github.com/SeismicSystems/seismic-foundry/pull/220) | Signed-read tx-context classification for sanvil. Added the requirement that a signed simulation declare `signed_read = true`. |

It is the 0x6A tx-context line of work, not the gas changes and not SEI-369.

It went unnoticed for nine days because every seismic-alloy PR in that window was already red
on the same job from the SEI-369 response-envelope format mismatch. Same job, louder error.
Landing that fix is what exposed this one.

seismic-viem was never affected: it already builds a proper twin in `estimateShieldedGas`.
Only the Rust provider missed the update.

## Why the one-line fix is wrong

The obvious patch is to flip the flag on the clone:

```rust
tx_for_estimate.seismic_elements.signed_read = true;
```

That makes things worse. `signed_read` is bound into the AEAD's additional authenticated data.
The calldata was already encrypted by `SeismicElementsFiller` with `signed_read = false` in the
AAD, so flipping the flag changes the AAD the node computes and the GCM tag no longer verifies.

You would trade `signed call must set signed_read=true` for a decryption failure — same red,
worse error, and a harder one to diagnose.

The twin therefore needs its own elements and its own ciphertext, not a mutated copy of the
write's.

## The change

68 lines across two files: `crates/network/src/fillers.rs` and `crates/provider/src/builder.rs`.

1. **Capture the plaintext in `prepare`.** Re-encrypting needs plaintext, and by `fill` time the
   elements filler has already encrypted. Every filler's `prepare` runs before any filler's
   `fill`, so `SeismicGasFiller::prepare` still sees plaintext calldata. It captures it and
   carries it through `Fillable`.
2. **Plumb the keys to the gas filler.** `tee_pubkey` and `provider_secret_key` lived only on
   `SeismicElementsFiller`. The builder already pulls the secret key out for the decrypt layer,
   so it now passes both to the gas filler via `with_encryption`.
3. **Build the twin in `fill`.** Fresh elements with `signed_read = true` and a fresh encryption
   nonce, then re-encrypt the captured plaintext under that metadata before signing and sending
   to `eth_estimateGas`.

The real write is untouched: it keeps `signed_read = false`, its original elements and its
original ciphertext.

This mirrors what seismic-viem does in `sendShielded.ts` — a separate `signedRead: true`
metadata, its own `client.encrypt`, and the write built from the original metadata.

## What to scrutinise

Ranked by how much damage a wrong assumption does.

**The ordering assumption.** The whole fix rests on `prepare` running before any `fill`, so that
the gas filler sees plaintext. I read that off alloy's `JoinFill` behaviour and confirmed it
empirically — the twin decrypts, which it could not if the captured bytes were ciphertext. If a
future alloy bump changes filler scheduling, this breaks in the sense that the estimate would
encrypt already-encrypted bytes. Worth a second opinion on whether that ordering is contractual
or incidental.

**The fresh nonce.** The twin gets a new `encryption_nonce` so it never shares a `(key, nonce)`
pair with the write. This is the same hazard SEI-369 fixed on the response path. If you disagree
that a fresh nonce is needed here, say so — the twin and the write encrypt the same plaintext,
so reuse would produce identical ciphertext rather than leaking a XOR, but I would rather not
rely on that staying true.

**The write really is untouched.** I assert it; a reviewer should check it. The twin is built on
a clone (`tx_for_estimate`), and the mutation happens only on that clone.

**The non-seismic path.** The twin logic is behind `if seismic_estimate.is_seismic()`, so
transparent transactions take the old route unchanged. Worth confirming that guard is in the
right place.

**No new test.** I verified against the existing suite rather than adding a regression test for
the twin specifically. A test asserting the estimate request carries `signed_read = true` would
stop this recurring.

## Testing, and the environment

`cargo test -p seismic-alloy-provider` against a locally built sanvil that includes foundry #220:
**34 passed, 0 failed**. Twelve were failing before. `cargo +nightly fmt --all --check` and
`RUSTFLAGS="-D warnings" cargo check --all-targets` are clean, and seismic-alloy-consensus still
passes 35.

Not done: no regression test for the twin itself, and I have not run this against reth — only
against sanvil. reth enforces the same rule via `ensure_signed_read_request`, so it should behave
identically, but that is reasoning rather than a run.

### Building locally on this machine

`openssl-sys` fails to build here. Debian puts `opensslconf.h` in an architecture-specific
directory that the crate's probe does not search, and the nix toolchain does not search
`/usr/lib/x86_64-linux-gnu` for the libraries.

```bash
export C_INCLUDE_PATH=/usr/include/x86_64-linux-gnu
export LIBRARY_PATH=/usr/lib/x86_64-linux-gnu
export OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu
export OPENSSL_INCLUDE_DIR=/usr/include
```

To *run* the tests, `LD_LIBRARY_PATH` must point at a directory holding only `libssl.so.3` and
`libcrypto.so.3`. Pointing it at the whole system library directory breaks the nix-built binaries
and nix-python's `ssl` module.

None of this is committed — it is local environment setup, but anyone building seismic-alloy on
this box will hit it.

### CI expectations

`integration-test (nightly)` should go green with this. `integration-test (stable)` will stay red
until a stable sfoundry release ships the SEI-369 response envelope; that job is
`allow-failure: true` and the workflow calls it "informational during coordinated breaking
releases".

## Open question

Do you want a regression test asserting the estimate carries `signed_read = true` before this
merges, or is landing the fix first fine?
