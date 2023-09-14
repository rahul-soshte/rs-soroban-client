//! A rust client library 
//! for interacting with Soroban smart contracts on the stellar blockchain
pub mod http_client;

pub use stellar_baselib::*;
pub use http_client::{HTTPClient, VERSION};
