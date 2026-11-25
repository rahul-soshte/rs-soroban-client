# Contract client tests

Run the client tests and check the feature-disabled build:

```sh
cargo test --features contract-spec --lib --tests
cargo check --no-default-features
```

## Generic `Val` red-phase tests

These tests specify explicit `ScVal` round trips through generic `Val`, including
structs, unions, containers, and event parameters. They intentionally fail until
generic-value decoding is implemented; the converter is unchanged.

```sh
cargo test --features contract-spec --test generic_val
cargo test --features contract-spec --test contract_client generic_val
```

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
three SEP-48 data formats, including events with shared topic prefixes. It requires
access to public testnet RPC and Friendbot.

Use the printed contract ID to run the example:

```sh
cargo run --features contract-spec --example typed_contract -- CONTRACT_ID
```

The fixture declares SEP-41 to test claim discovery but does not implement a token
interface.

After deploying the updated fixture, use its printed contract ID to check generic
`Val` against testnet. This opt-in test first verifies the raw RPC result, then
exercises the currently failing typed-client decoding:

```sh
GENERIC_VAL_CONTRACT_ID=CONTRACT_ID \
    cargo test --features contract-spec --test testnet_generic_val -- --ignored --nocapture
```
