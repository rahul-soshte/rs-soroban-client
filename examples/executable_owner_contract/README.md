# Executable owner contract (CAP-85)

Source for `examples/executable_owner_contract.wasm`, used by the `p28_e2e`
example. It owns CAP-85 executable reference entries: `publish(tag, wasm_hash)`
writes the tag entry via `env.executable_refs().set(...)`, and `resolve(tag)`
reads it back.

This crate is standalone (not a workspace member) and is not built by the main
project. To rebuild the wasm you need soroban-sdk 28+ and stellar-cli 25.2+:

```bash
cd examples/executable_owner_contract
stellar contract build
cp target/wasm32v1-none/release/executable_owner_contract.wasm ../executable_owner_contract.wasm
```
