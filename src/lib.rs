#![warn(missing_docs)]
//! A rust client library for interacting with Soroban smart contracts on the stellar blockchain
//! through the [Stellar RPC]
//!
//!
//!# Example: Create account on testnet and fetch balance using simulation
//!```rust
//! # use std::rc::Rc;
//! # use std::cell::RefCell;
//! # use soroban_client::*;
//! # use soroban_client::account::Account;
//! # use soroban_client::address::Address;
//! # use soroban_client::address::AddressTrait;
//! # use soroban_client::contract::Contracts;
//! # use soroban_client::contract::ContractBehavior;
//! # use soroban_client::network::Networks;
//! # use soroban_client::network::NetworkPassphrase;
//! # use soroban_client::xdr::Int128Parts;
//! # use soroban_client::xdr::ScVal;
//! # use soroban_client::xdr::int128_helpers::i128_from_pieces;
//! # use soroban_client::keypair::Keypair;
//! # use soroban_client::keypair::KeypairBehavior;
//! # use soroban_client::transaction::AccountBehavior;
//! # use soroban_client::transaction::TransactionBuilderBehavior;
//! # use soroban_client::transaction_builder::TransactionBuilder;
//! #[tokio::main]
//!async fn main() {
//!    let rpc = Server::new("https://soroban-testnet.stellar.org", Options::default()).unwrap();
//!
//!    let native_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
//!    let native_sac = Contracts::new(native_id).unwrap();
//!
//!    // Generate an account
//!    let kp = Keypair::random().unwrap();
//!
//!    // Create the account using friendbot
//!    let account = rpc.request_airdrop(&kp.public_key()).await.unwrap();
//!
//!    let source_account = Rc::new(RefCell::new(
//!        Account::new(&kp.public_key(), &account.sequence_number()).unwrap(),
//!    ));
//!
//!   
//!    let account_address = Address::new(&kp.public_key()).unwrap();
//!    let tx = TransactionBuilder::new(source_account, Networks::testnet(), None)
//!        .fee(1000u32)
//!        .add_operation(
//!            native_sac.call(
//!                "balance",
//!                Some(vec![account_address.to_sc_val().unwrap()])))
//!        .build();
//!
//!    let response = rpc.simulate_transaction(&tx, None).await.unwrap();
//!    if let Some((ScVal::I128(Int128Parts { hi, lo }), _auth)) = response.to_result() {
//!        // Divide to convert from stroops to XLM
//!        let balance = i128_from_pieces(hi, lo) / 10000000;
//!        println!("Account {} has {} XLM", kp.public_key(), balance);
//!    }
//!}
//!```
//!
//!# Example: Fetching last 3 transfer events from the native asset contract on testnet
//!```rust
//! # use soroban_client::*;
//! # use soroban_client::network::Networks;
//! # use soroban_client::network::NetworkPassphrase;
//! # use soroban_client::xdr::ScVal;
//! # use soroban_client::soroban_rpc::*;
//! # use soroban_client::xdr::ScSymbol;
//! # use soroban_client::xdr::ScString;
//!#[tokio::main]
//!async fn main() {
//!    let rpc = Server::new("https://soroban-testnet.stellar.org", Options::default()).unwrap();
//!
//!    let native_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
//!
//!    let response = rpc.get_latest_ledger().await.unwrap();
//!    let ledger = response.sequence;
//!
//!    let transfer = ScVal::Symbol(ScSymbol("transfer".try_into().unwrap()));
//!    let native = ScVal::String(ScString("native".try_into().unwrap()));
//!    let events = rpc
//!        .get_events(
//!            Pagination::From(ledger - 100),
//!            vec![EventFilter::new(crate::soroban_rpc::EventType::All)
//!                .contract(native_id)
//!                .topic(vec![
//!                    Topic::Val(transfer),
//!                    Topic::Any, // From account
//!                    Topic::Any, // To account
//!                    Topic::Val(native),
//!                ])],
//!            3
//!        )
//!        .await
//!        .unwrap();
//!
//!    println!("{:?}", events);
//!}
//! ```
//! [Stellar RPC]: https://developers.stellar.org/docs/data/rpc

/// Current version of this crate
pub static VERSION: &str = env!("CARGO_PKG_VERSION");
pub use crate::server::*;
/// Error module
pub mod error;
/// Soroban bindings
pub mod soroban_rpc;
/// Transaction module
pub mod transaction;
pub use stellar_baselib::*;

// for now, not public
//mod contract_spec;
mod async_utils;
mod friendbot;
mod jsonrpc;
mod server;

#[cfg(test)]
mod tests;
