# Seismic gas-payment selector (fresh-chain format)

The fresh chain keeps EIP-2718 transaction type **`0x4A` (74)**. There is no new
format-version field. `message_version` retains its existing signing-mode semantics
(`0` for RLP signing; values at least `2` for EIP-712). The selector is public,
signed fee metadata, not encrypted calldata or a balance disclosure.

Every new-format `TxSeismic` has a mandatory top-level `gas_payment`. It is not part
of `TxSeismicElements`, and the calldata-encryption AAD is unchanged.

## Canonical RLP

The unsigned fields, in order, are:

```text
[chainId, nonce, gasPrice, gasLimit, gasPayment, to, value,
 encryptionPubkey, encryptionNonce, messageVersion, recentBlockHash,
 expiresAtBlock, signedRead, input, authorizationList]
```

The signed form appends the existing signature fields. `gasPayment` is one nested
RLP list `[kind, token]` with canonical integer and byte-string encoding:

| Choice | kind | token bytes | Example RLP |
| --- | --- | --- | --- |
| Auto | 0 | empty | `c28080` |
| Native | 1 | empty | `c20180` |
| Token(address) | 2 | exactly 20 bytes, nonzero address | `d60294` followed by the address |

Reject unknown tags, wrong token lengths, zero-address Token, tokens attached to
Auto/Native, missing/extra fields, and noncanonical RLP. The old raw layout is not
accepted and must never acquire an implicit Auto after signature verification.
Standard Ethereum transaction formats/signatures are unchanged.

## JSON and request defaults

The `gasPayment` property uses these exact tagged objects:

```json
{"type":"auto"}
{"type":"native"}
{"type":"token","token":"0x1111111111111111111111111111111111111111"}
```

Reject unknown tags/fields, a missing Token address, and zero-address Token.
Transaction-construction requests may omit `gasPayment`; builders resolve that to
Auto **before signing**. Decoded/signed transaction JSON must include the field.
Non-Auto selection on a standard transaction request is rejected, not treated as
an unsigned fee hint.

In Rust, use `SeismicTransactionRequest::gas_payment(...)` to select the fee asset.
`From<TxSeismic>` and the typed/envelope conversions preserve the selector and
encryption elements. The generic `from_transaction` / `from_transaction_with_sender`
helpers copy only standard Ethereum fields because the `Transaction` trait does not
expose Seismic metadata; they leave encryption elements absent and payment as Auto.
Do not use those generic helpers alone to reconstruct a Seismic request.

## EIP-712

Add a `GasPayment` type:

```text
GasPayment(uint8 kind,address token)
```

The `TxSeismic` schema inserts `GasPayment gasPayment` immediately after
`uint64 gasLimit`. Its nested message contains both `kind` and `token`.
Auto/Native require the zero address as the token sentinel; Token requires a
nonzero address. The signing schema must include this field. No version field is
added and the existing domain/signing-mode selection remains unchanged.

## Execution semantics

- Auto: native first, then eligible active registry tokens in insertion order.
- Native: native only, with no registry reads or token fallback.
- Token: exactly the selected registered token, even when native could fund gas;
  unknown, inactive, unsupported, incompatible, or insufficient tokens fail.

Native currency funds transaction value. One asset must cover the entire maximum
gas cost; fees cannot be split across balances. Registry-controlled decimals in
0–18 scale base units under the fixed one-whole-token-per-whole-native-unit policy.
The selector never supplies a mapping slot, mode, precision, or exchange rate.

All fresh-chain clients must use this layout from genesis. Supporting the previous
Seismic raw format and existing-network migration are out of scope.
