#![no_std]
use soroban_sdk::{contract, contractimpl, BytesN, Env, String};

#[contract]
pub struct Owner;

#[contractimpl]
impl Owner {
    /// Publish (or update) the executable reference entry `tag` -> `wasm_hash` (CAP-85).
    pub fn publish(env: Env, tag: String, wasm_hash: BytesN<32>) {
        env.executable_refs().set(&tag, &wasm_hash);
    }

    /// Read back the wasm hash for `tag`, if published.
    pub fn resolve(env: Env, tag: String) -> Option<BytesN<32>> {
        env.executable_refs().get(&tag)
    }
}
