# Native contract calls

## Enable the feature

Enable `contract-spec` (Rust 1.93+). Add Serde if you use custom Rust structs or
enums:

```toml
[dependencies]
soroban-client = { version = "0.8", features = ["contract-spec"] }
serde = { version = "1", features = ["derive"] }
```

## Without and with `contract-spec`

These examples simulate a deployed function `add(a: u32, b: u32) -> u32` using an
existing `server`, `contract_id`, `source_account`, and `network_passphrase`.

### Without `contract-spec`

```rust,ignore
use soroban_client::{
    contract::{ContractBehavior, Contracts},
    transaction::{TransactionBuilder, TransactionBuilderBehavior},
    xdr::ScVal,
};

let contract = Contracts::new(contract_id)?;
let mut source = source_account.clone();
let tx = TransactionBuilder::new(&mut source, network_passphrase, None)
    .fee(1000u32)
    .add_operation(contract.call(
        "add",
        Some(vec![ScVal::U32(2), ScVal::U32(3)]),
    ))
    .build();

let simulation = server.simulate_transaction(&tx, None).await?;
let (result, _) = simulation.to_result().ok_or("missing result")?;
let value = match result {
    ScVal::U32(value) => value,
    _ => return Err("unexpected return type".into()),
};
assert_eq!(value, 5);
```

### With `contract-spec`

```rust,ignore
use soroban_client::contract_client::Client;

let client = Client::new(&server, contract_id).await?;
let value: u32 = client.call("add", &(2u32, 3u32))?
    .simulate(&source_account, network_passphrase).await?;
assert_eq!(value, 5);
```

The client validates argument counts and types against the contract's embedded
interface, converts the inputs, and decodes the result. Simulation does not submit
a transaction; use the preparation flow below to execute on-chain.

## Arguments and return values

Pass arguments as a positional tuple: `()` for no arguments, `(value,)` for one,
or `(a, b)` for two. You can also pass a struct whose field names match the
function's inputs:

```rust,ignore
#[derive(serde::Serialize)]
struct AddArgs { a: u32, b: u32 }

let call = client.call("add", &AddArgs { a: 2, b: 3 })?;
```

| Contract type | Rust representation |
| --- | --- |
| bool, signed/unsigned 32/64/128-bit integer | Corresponding primitive |
| signed/unsigned 256-bit integer | `soroban_client::contract_spec::{I256, U256}` |
| string, symbol, address | `String` / `&str`; addresses use StrKey strings |
| bytes, bytesN | `Vec<u8>` / `[u8; N]` |
| void | `()` |
| timepoint, duration | `u64` seconds |
| option, vector, tuple | `Option<T>`, `Vec<T>`, Rust tuple |
| map | `BTreeMap<K, V>` / `HashMap<K, V>`, including tuple and struct keys |
| struct, tuple struct | Rust struct deriving `Serialize` / `Deserialize` |
| union | Externally tagged Serde enum with unit/newtype/tuple variants |
| numeric enum | Serde enum; variant names map to the contract's numeric values |

Use Serde rename attributes when Rust names differ from contract names. Numeric
enums use the specification's discriminants, not Rust variant positions.
For function outputs declared as `Result<T, E>`, request `T`; failures are returned
through `CallError`.

### Maps with composite keys

Pass a native map directly; no wrapper or manual key encoding is needed:

```rust,ignore
use std::collections::BTreeMap;

let values = BTreeMap::from([((1u32, 2u32), 10i128)]);
let result: BTreeMap<(u32, u32), i128> = client.call("echo_map", &(&values,))?
    .simulate(&source_account, network_passphrase).await?;
assert_eq!(result, values);
```

`HashMap` works the same way; input and output collections need not be the same
type. Keys can use supported tuples, structs, enums, or nested containers. Maps
also work inside arguments and return types such as structs and unions. The
adapter sorts encoded keys in Stellar's order and rejects duplicate encoded keys.

## Handle contract errors

Derive `Deserialize` on your error enum and match the contract's variant names:

```rust,ignore
use soroban_client::contract_client::CallError;

#[derive(Debug, serde::Deserialize)]
enum ArithmeticFailure { Overflow }

let call = client.call("add", &(u32::MAX, 1u32))?;
match call.try_simulate::<u32, ArithmeticFailure>(&source_account, network_passphrase).await {
    Ok(value) => println!("{value}"),
    Err(CallError::Contract(ArithmeticFailure::Overflow)) => println!("overflow"),
    Err(CallError::UnknownContract(error)) => println!("unmapped contract error {}", error.code),
    Err(other) => return Err(other.into()),
}
```

Unknown, ambiguous, or unmapped contract errors retain their numeric code and
available symbolic matches. Without an application enum, `simulate::<T>` and
`prepare::<T>` return structured `ContractFailure` values in `CallError::Contract`.
If a failed simulation has no usable structured diagnostics, it returns
`CallError::Simulation` instead of a typed contract variant.

For errors used as ordinary arguments, fields, containers, or event parameters,
use `xdr::ScError` for a generic `Error` type. For a declared contract-error enum,
derive `Serialize`/`Deserialize` on your matching Rust enum. Generic errors retain
unknown contract codes and host error classes; declared enums reject unknown
codes. These values are data, not `CallError::Contract`.

Use ordinary Rust `Result<T, E>` for `Result` arguments and nested fields,
containers, or event parameters. `Ok` encodes as its payload; `Err` encodes as
an error value, with `E` using `ScError` or a matching declared error enum.
Top-level invocation outputs still return `T` on success and `CallError` on
failure. Because the wire has no extra `Result` tag, `Ok` payloads that encode
directly as an error (including `Ok(Err(...))`) are rejected. Errors inside a
successful container remain valid.

## Submit a transaction

```rust,ignore
use soroban_client::transaction::TransactionBehavior;

let call = client.call("add", &(2u32, 3u32))?;
let prepared = call.prepare::<u32>(&source_account, network_passphrase).await?;
let mut transaction = prepared.transaction;
transaction.sign(&signers);
let sent = server.send_transaction(transaction).await?;
let confirmed = server.wait_transaction(&sent.hash, std::time::Duration::from_secs(90))
    .await.map_err(|(error, _)| error)?;
let actual = call.decode_transaction::<u32, ArithmeticFailure>(&confirmed)?;
```

`prepared.simulated_value` is a prediction. Decode the confirmed response for the
same invocation to obtain its actual result.

- `try_prepare::<T, E>` and `try_simulate_with_options::<T, E>` accept
  `InvocationOptions` for fees, timeout, and simulation/auth settings.
- Source accounts are not advanced locally. Refresh them before sequential
  submissions and coordinate sequence numbers for concurrent transactions.
- Sign non-source-account authorizations yourself. Archived state returns
  `RestorationRequired`; restoration is not automatic.
- Use `operation()` or `transaction()` for custom transaction workflows.

## Decode contract events

For the `Added { value: u32 }` event emitted by the fixture's `add` function:

```rust,ignore
#[derive(Debug, serde::Deserialize)]
struct Added { value: u32 }

let (_, rows) = confirmed.to_events().ok_or("missing events")?;
for event in rows.iter().flatten() {
    let added: Added = client.decode_event(event)?;
    println!("{}", added.value);
}
```

Pass an `EventResponse` from `Server::get_events` the same way. Successful
simulation contract events are also accepted, but do not represent confirmed
emissions. Topic and data parameters are combined into the Rust struct, using
the native types listed above, including composite-key maps and large integers.

The client checks the emitting contract ID and matches the complete payload, not
just its topic prefix. If multiple definitions match, it returns
`EventDecodeError::Ambiguous`. Select a definition explicitly when needed:

```rust,ignore
let added: Added = client.decode_event_as("Added", event)?;
```

Explicit selection still validates the prefix and parameter types. Malformed
payloads, system/diagnostic events, and events marked as unsuccessful are rejected.

## Read the interface and metadata

`Client::new` fetches the contract's Wasm and reads its interface (SEP-48), metadata
(SEP-46), and declared SEP claims (SEP-47). Reuse that snapshot:

```rust,ignore
let spec = client.spec();
let metadata = client.metadata();
let claims = client.sep_claims()?;
```

For discovery without a client, use the server readers. Each fetches independently:

```rust,ignore
let spec = server.get_contract_spec(contract_id).await?;
let metadata = server.get_contract_metadata(contract_id).await?;
let claims = server.get_contract_sep_claims(contract_id).await?;
```

Metadata preserves duplicate keys. SEP claims combine all `sep` entries and
deduplicate numbers in declaration order; malformed identifiers return an error.
Claims are not proof that a contract implements a SEP.

Unit `()` uses `Void` on the wire, including when the spec declares `Tuple([])`.
Pass a single unit argument as `&((),)`, not `&()` (which means no arguments).
`Option<()>` cannot distinguish `Some(())` from `None` on the wire.

## Limitations

- Recreate the client after a contract upgrade to refresh its interface.
- Built-in Stellar Asset Contracts have no Wasm; discovery returns
  `ContractCodeNotFound`. Missing or malformed interfaces also return errors.
- Internally tagged/untagged enums and union struct variants are unsupported.
- Generic `val` uses explicit `xdr::ScVal` for arguments, results and event
  parameters, including inside containers. Primitive types are not inferred:
  `ScVal::String` and `ScVal::Symbol`, for example, keep distinct wire tags.
- The interface is a contract's self-description, not verification of its behavior.

See [`examples/typed_contract.rs`](../examples/typed_contract.rs) for a runnable
testnet example and [the testing instructions](../tests/README.md) to deploy its
fixture.
