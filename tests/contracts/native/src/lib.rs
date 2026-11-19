#![no_std]
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contractmeta, contracttype, Env, String,
    I256, U256,
};

contractmeta!(key = "fixture", val = "native-client-v1");
// Deliberately unverified claims: this fixture is NOT a SEP-41 token.
contractmeta!(key = "sep", val = "41,40");
mod additional_claims {
    soroban_sdk::contractmeta!(key = "sep", val = "48,41");
}

#[contracttype]
#[derive(Clone)]
pub struct Record {
    pub z: i128,
    pub a: String,
}

#[contracttype]
#[derive(Clone)]
pub enum Choice {
    Empty,
    One(u128),
    Pair(u32, String),
}

#[contracttype]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum Color {
    Red = 3,
    Green = 7,
    Blue = 11,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ArithmeticFailure {
    Overflow = 7,
}

#[contractevent]
pub struct Added {
    pub value: u32,
}

#[contract]
pub struct NativeFixture;

#[contractimpl]
impl NativeFixture {
    /// Add two integers, returning a typed error on overflow.
    pub fn add(env: Env, a: u32, b: u32) -> Result<u32, ArithmeticFailure> {
        let value = a.checked_add(b).ok_or(ArithmeticFailure::Overflow)?;
        Added { value }.publish(&env);
        Ok(value)
    }
    pub fn echo_record(value: Record) -> Record {
        value
    }
    pub fn echo_choice(value: Choice) -> Choice {
        value
    }
    pub fn echo_color(value: Color) -> Color {
        value
    }
    pub fn echo_i128(value: i128) -> i128 {
        value
    }
    pub fn echo_u128(value: u128) -> u128 {
        value
    }
    pub fn echo_i256(value: I256) -> I256 {
        value
    }
    pub fn echo_u256(value: U256) -> U256 {
        value
    }
}
