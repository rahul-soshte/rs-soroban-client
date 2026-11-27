# Contract client tests

Run the client tests and check the feature-disabled build:

```sh
cargo test --features contract-spec --lib --tests
cargo check --no-default-features
```

## Generic `Val`

These tests specify explicit `ScVal` round trips through generic `Val`, including
structs, unions, containers, and event parameters. XDR tags are preserved rather
than inferred from native primitives.

```sh
cargo test --features contract-spec --test generic_val
cargo test --features contract-spec --test contract_client generic_val
```

## Errors as ordinary data

These tests distinguish error-valued arguments, fields, containers and events
from invocation failures. Generic `ScError` values preserve their type and code;
declared contract errors map to caller-defined Serde enums.

```sh
cargo test --features contract-spec --test error_data
cargo test --features contract-spec --test contract_client error_data
```

## Input and nested `Result`

These tests cover native `Ok`/`Err` inputs and results inside records, vectors,
map values, options and events. The SDK encodes `Ok` as its payload and `Err` as
an error value, without an extra tag. `Ok` payloads that encode directly as an
error, such as `Ok(error)` and `Ok(Err(...))`, are rejected. Top-level invocation
errors keep their existing API.

```sh
cargo test --features contract-spec --test result_data
cargo test --features contract-spec --test contract_client result_data
```

After deploying the updated fixture, run the live checks. Each verifies raw RPC
behavior before verifying native conversion:

```sh
RESULT_DATA_CONTRACT_ID=CONTRACT_ID \
    cargo test --features contract-spec --test testnet_result_data -- --ignored --nocapture
```

## Zero-element tuples

An explicit SDK `()` can have spec type `Tuple([])` while using `Void` on the
wire. These tests cover inputs, outputs, record fields and event parameters.
`Option<()>` cannot distinguish `Some(())`
from `None` on the wire, so a lossless round trip is not required.

```sh
cargo test --features contract-spec --test zero_tuple
ZERO_TUPLE_CONTRACT_ID=CONTRACT_ID \
    cargo test --features contract-spec --test testnet_zero_tuple -- --ignored --nocapture
```

Use the contract ID printed after deploying the updated fixture below. Live tests
verify its SDK-generated spec and raw RPC values before native conversion.

## Testnet deployment test

Build the contract fixture, then run the opt-in deployment test:

```sh
cargo build --manifest-path tests/contracts/native/Cargo.toml \
    --target wasm32v1-none --release
NATIVE_FIXTURE_WASM=tests/contracts/native/target/wasm32v1-none/release/native_client_fixture.wasm \
    cargo test --features contract-spec --test testnet_contract_client \
    -- --ignored --nocapture
```

The test funds a disposable account, deploys the fixture, confirms a native call,
checks typed errors and metadata, and round-trips user-defined types, composite-key
maps, and 128/256-bit integers. It also decodes confirmed and RPC events in all
three SEP-48 data formats, including events with shared topic prefixes. Submitted
calls also verify an `Err` argument/event, an error-valued return record/event,
and a generic `Val` return/event with explicit XDR tags. A submitted unit call
verifies a zero-element tuple argument, return value and event parameter.
It requires
access to public testnet RPC and Friendbot.

Use the printed contract ID to run the example:

```sh
cargo run --features contract-spec --example typed_contract -- CONTRACT_ID
```

The fixture declares SEP-41 to test claim discovery but does not implement a token
interface.

After deploying the updated fixture, use its printed contract ID to check generic
`Val` against testnet. This opt-in test first verifies the raw RPC result, then
verifies typed-client decoding:

```sh
GENERIC_VAL_CONTRACT_ID=CONTRACT_ID \
    cargo test --features contract-spec --test testnet_generic_val -- --ignored --nocapture
```

For error-valued data, use the updated fixture's contract ID. Each test first
checks the raw RPC result, then verifies native conversion:

```sh
ERROR_DATA_CONTRACT_ID=CONTRACT_ID \
    cargo test --features contract-spec --test testnet_error_data -- --ignored --nocapture
```
