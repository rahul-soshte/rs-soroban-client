#![no_std]
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contractmeta, contracttype, Address, Env,
    Map, String, Val, Vec, I256, U256,
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
pub struct MapKey {
    pub z: u32,
    pub a: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct MapContainer {
    pub values: Map<MapKey, Vec<Option<u128>>>,
}

#[contracttype]
#[derive(Clone)]
pub struct ValRecord {
    pub value: Val,
    pub values: Vec<Val>,
    pub keyed: Map<Val, Val>,
    pub optional: Option<Val>,
}

#[contracttype]
#[derive(Clone)]
pub enum ValChoice {
    One(Val),
    Many(Vec<Val>),
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

#[contractevent(data_format = "single-value")]
pub struct GenericChanged {
    pub value: Val,
}

#[contractevent(topics = ["shared"], data_format = "single-value")]
pub struct NativeCount {
    #[topic]
    pub owner: Address,
    pub value: u128,
}

#[contractevent(topics = ["shared"], data_format = "single-value")]
pub struct NativeText {
    #[topic]
    pub owner: Address,
    pub text: String,
}

#[contractevent(data_format = "vec")]
pub struct NativeValues {
    #[topic]
    pub owner: Address,
    pub count: u128,
    pub values: Map<(u32, u32), i128>,
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
    pub fn echo_map(value: Map<(u32, u32), i128>) -> Map<(u32, u32), i128> {
        value
    }
    pub fn echo_struct_map(value: Map<MapKey, U256>) -> Map<MapKey, U256> {
        value
    }
    pub fn echo_nested_map(value: MapContainer) -> MapContainer {
        value
    }
    pub fn echo_val(env: Env, value: Val) -> Val {
        GenericChanged { value }.publish(&env);
        value
    }
    pub fn echo_vals(value: Vec<Val>) -> Vec<Val> {
        value
    }
    pub fn echo_val_map(value: Map<Val, Val>) -> Map<Val, Val> {
        value
    }
    pub fn echo_option_val(value: Option<Val>) -> Option<Val> {
        value
    }
    pub fn echo_val_record(value: ValRecord) -> ValRecord {
        value
    }
    pub fn echo_val_choice(value: ValChoice) -> ValChoice {
        value
    }
    pub fn emit_events(env: Env, count: u128, values: Map<(u32, u32), i128>) {
        let owner = env.current_contract_address();
        NativeCount {
            owner: owner.clone(),
            value: count,
        }
        .publish(&env);
        NativeText {
            owner: owner.clone(),
            text: String::from_str(&env, "hello"),
        }
        .publish(&env);
        NativeValues {
            owner,
            count,
            values,
        }
        .publish(&env);
    }
}
