# Rust Soroban Client Library

![Crates.io](https://img.shields.io/crates/v/soroban-client)
![Crates.io](https://img.shields.io/crates/l/soroban-client)
![Crates.io](https://img.shields.io/crates/d/soroban-client)
![publish workflow](https://github.com/rahul-soshte/rs-soroban-client/actions/workflows/publish.yml/badge.svg)

<img src="img/rust-soroban-client-logo.png" alt="drawing" width="300"/>

A Rust client library for interacting with Soroban smart contracts on the Stellar blockchain

**This project is currently in production and is compatible with Protocol 23 and you can use it for buidling and signing transactions that involve interacting with Soroban and also supports all stellar classic operations.**

## Quickstart

Add this to your Cargo.toml:

```toml
[dependencies]
soroban-client = "0.5.1"
```

And this to your code:

```rust
use soroban_client::*;
```

## Crate Docs

[Docs Link](https://docs.rs/soroban-client/latest/soroban_client/)

## Description

**The library is composed of 3 components**:

1. **[rs-stellar-xdr](https://github.com/stellar/rs-stellar-xdr)**: a low-level library for encoding/decoding XDR data. This has already been developed by the Stellar Core team.
2. **[rs-stellar-base](https://github.com/rahul-soshte/rs-stellar-base)**: a library that offers a comprehensive set of functions for reading, writing, hashing, and signing primitive XDR constructs utilized in the Stellar network. It provides a nice abstraction for building and signing transactions.
3. **[rs-soroban-client](https://github.com/rahul-soshte/rs-soroban-client)**: A high-level rust library that serves as client-side API for the Soroban Environment. Useful for communicating with a Soroban RPC server.

This library will enable developers to seamlessly integrate Soroban functionality into their Rust-based applications and services. Most of the groundwork has already been laid by the Stellar team by building the xdr library and  rust stellar strkey implementation. This particular library has been the missing piece for soroban and the rust community at large in the stellar ecosystem.

## Running Examples

```bash
cargo run --example create_account
cargo run --example payment
cargo run --example deploy
```

## Sample Demo of the library

[Demo Link](sdemo/src/main.rs)


## Getting Help

Join the [discord server](https://discord.gg/mH9R2mw9tP) to chat with the community!

## Practical Use Case

Suppose someone wants to build a trading bot targeting a DEX built on Soroban itself. This bot executes a large number of trades within a short period, often leveraging market inefficiencies and price discrepancies.  A Rust client library for Soroban would provide the person with a performant toolset to build trading algorithms, interact with the Stellar network, and execute trades with minimal latency.


## Authors

Rahul Soshte ([Twitter](https://twitter.com/RahulSoshte))
