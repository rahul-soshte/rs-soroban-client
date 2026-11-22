# Migrating soroban-client from 0.7.x to 0.8.0

0.8.0 is an intentional breaking release for transaction parsing and corrected
base-crate field types. These changes must not be shipped as a `0.7.x` patch.
Protocol compatibility remains Protocol 30.

```toml
[dependencies]
soroban-client = "0.8.0"
# If used directly too, keep the base crate on the same release line:
stellar-baselib = "0.8.0"
```

## Update envelope parsing

Before:

```rust,ignore
let tx = Transaction::from_xdr_envelope(encoded, network);
rpc.send_transaction(tx).await?;
```

After, applications accepting both ordinary transactions and fee bumps can pass
the parsed enum directly to submission:

```rust,no_run
use soroban_client::{Server, transaction::{Transaction, TransactionBehavior}};

async fn forward(rpc: &Server, encoded: &str, network: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    let parsed = Transaction::from_xdr_envelope(encoded, network)?;
    rpc.send_transaction(parsed).await?;
    Ok(())
}
```

For ordinary-only callers, use
`Transaction::from_xdr_envelope(encoded, network)?.into_transaction()?`.
Malformed XDR returns a typed `TransactionError`; a valid fee bump requested as
an ordinary transaction returns `UnexpectedFeeBump` instead of losing its outer
envelope. Custom `TransactionBehavior` implementations must update their parser
return type to `Result<ParsedTransaction, TransactionError>` as well.

## Update direct transaction-field usage

- `min_account_sequence_age`: `Option<u32>` becomes `Option<u64>`.
- `extra_signers`: `Option<Vec<AccountId>>` becomes `Option<Vec<SignerKey>>`.
- Parsed `min_account_sequence` is a decimal string, not base64 XDR.
- Builders leave unused V2-only fields as `None`. Explicit `Some(0)` or empty
  signer vectors select V2; an all-zero parsed V2 remains V2 during serialization.
- V0 serialization returns an error for unsupported muxed sources, V2
  preconditions or Soroban data instead of silently dropping fields.

Update typed wrappers and struct literals accordingly. Detailed conversion
examples are in the base crate's `MIGRATION.md`.

## What ordinary callers can keep

Existing ordinary construction, signing and `send_transaction(tx)` calls still
work. Submission now accepts `Transaction`, `FeeBumpTransaction` and
`ParsedTransaction`, owned or by shared reference. `TransactionEnvelopeSource` is
the new public submission conversion interface. Construction errors are exposed
as `transaction::FeeBumpError`.

`to_envelope` retains its boxed-error return type. New `try_to_envelope`,
`try_signature_base` and `try_hash` methods offer typed fallible alternatives;
the legacy infallible hash/signing APIs have not become panic-free.

Simulation, preparation and assembly remain ordinary-transaction APIs. For
Soroban, simulate/assemble, finish authorization and sign the inner transaction
before wrapping and signing the fee bump. Poll the **outer hash** for confirmation.

See `examples/fee_bump.rs` for the full positive testnet flow. Publication order
and local dependency handling are covered by the
[release checklist](release-0.8.0.md).
