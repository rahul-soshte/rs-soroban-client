//! A rust client library
//! for interacting with Soroban smart contracts on the stellar blockchain
pub static VERSION: &str = env!("CARGO_PKG_VERSION");
pub mod contract_spec;
pub mod error;
pub mod friendbot;
pub mod soroban_rpc;
pub use stellar_baselib::*;
pub mod server;
pub mod transaction;
pub use self::server::SUBMIT_TRANSACTION_TIMEOUT;

mod jsonrpc;

#[cfg(test)]
mod tests;
