//! Opt-in live acceptance test; see docs/contract-client.md for fixture build.
//! All invocation inputs and outputs are native values. Deployment reuses the
//! existing upload/create APIs and does not construct any ScVal either.
#![cfg(feature = "contract-spec")]

use serde::{Deserialize, Serialize};
use soroban_client::{
    account::AccountBehavior,
    address::{Address, AddressTrait},
    contract_client::{CallError, Client},
    contract_spec::{I256, U256},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    soroban_rpc::{GetTransactionResponse, TransactionStatus},
    transaction::{
        Transaction, TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior,
    },
    Options, Server,
};
use std::time::Duration;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    z: i128,
    a: String,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Choice {
    Empty,
    One(u128),
    Pair(u32, String),
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Color {
    Red,
    Green,
    Blue,
}
#[derive(Debug, PartialEq, Deserialize)]
enum ArithmeticFailure {
    Overflow,
}

async fn submit(
    server: &Server,
    mut tx: Transaction,
    key: &Keypair,
) -> Result<GetTransactionResponse, Box<dyn std::error::Error>> {
    tx.sign(std::slice::from_ref(key));
    let sent = server.send_transaction(tx).await?;
    let result = server
        .wait_transaction(&sent.hash, Duration::from_secs(90))
        .await
        .map_err(|(e, _)| e)?;
    if result.status != TransactionStatus::Success {
        return Err(format!("transaction {} failed: {:?}", sent.hash, result).into());
    }
    Ok(result)
}

#[tokio::test]
#[ignore = "deploys a contract to public testnet; requires NATIVE_FIXTURE_WASM"]
async fn deploy_and_call_with_native_values() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = std::fs::read(std::env::var("NATIVE_FIXTURE_WASM")?)?;
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let key = Keypair::random()?;
    let mut source = server.request_airdrop(&key.public_key()).await?;

    let upload = Operation::new()
        .upload_wasm(&wasm, None)
        .map_err(|e| format!("{e:?}"))?;
    let tx = TransactionBuilder::new(&mut source, Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(upload)
        .build();
    let result = submit(&server, server.prepare_transaction(&tx).await?, &key).await?;
    let hash: Vec<u8> = result
        .to_result_meta()
        .and_then(|(_, v)| v)
        .ok_or("missing upload hash")?
        .try_into()
        .map_err(|_| "invalid upload hash")?;
    let hash: [u8; 32] = hash.try_into().map_err(|_| "invalid hash length")?;

    source = server.get_account(&key.public_key()).await?;
    let create = Operation::new()
        .create_contract(&key.public_key(), hash, None, None, Vec::new())
        .map_err(|e| format!("{e:?}"))?;
    let tx = TransactionBuilder::new(&mut source, Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(create)
        .build();
    let result = submit(&server, server.prepare_transaction(&tx).await?, &key).await?;
    let id = Address::from_sc_val(
        &result
            .to_result_meta()
            .and_then(|(_, v)| v)
            .ok_or("missing contract address")?,
    )?
    .to_string();
    eprintln!("Deployed native fixture: {id}");

    let client = Client::new(&server, &id).await?;
    assert!(client
        .metadata()
        .entries
        .iter()
        .any(|e| e.key == "fixture" && e.value == "native-client-v1"));
    let claims = server.get_contract_sep_claims(&id).await?;
    assert!(claims.contains(&41) && claims.contains(&40) && claims.contains(&48));
    assert_eq!(
        client.spec().entries(),
        server.get_contract_spec(&id).await?.entries()
    );
    assert_eq!(client.metadata(), &server.get_contract_metadata(&id).await?);

    source = server.get_account(&key.public_key()).await?;
    let call = client.call("add", &(2u32, 3u32))?;
    let prepared = call.prepare::<u32>(&source, Networks::testnet()).await?;
    assert_eq!(prepared.simulated_value, 5);
    let confirmed = submit(&server, prepared.transaction, &key).await?;
    assert_eq!(
        call.decode_transaction::<u32, ArithmeticFailure>(&confirmed)?,
        5
    );

    source = server.get_account(&key.public_key()).await?;
    assert!(matches!(
        client
            .call("add", &(u32::MAX, 1u32))?
            .try_simulate::<u32, ArithmeticFailure>(&source, Networks::testnet())
            .await,
        Err(CallError::Contract(ArithmeticFailure::Overflow))
    ));

    macro_rules! echo {
        ($method:literal,$value:expr,$ty:ty) => {{
            let value: $ty = $value;
            let result: $ty = client
                .call($method, &(&value,))?
                .simulate(&source, Networks::testnet())
                .await?;
            assert_eq!(result, value);
        }};
    }
    echo!(
        "echo_record",
        Record {
            z: i128::MIN,
            a: "native Rust".into()
        },
        Record
    );
    echo!("echo_choice", Choice::Empty, Choice);
    echo!("echo_choice", Choice::One(u128::MAX), Choice);
    echo!("echo_choice", Choice::Pair(7, "hello".into()), Choice);
    echo!("echo_color", Color::Green, Color);
    echo!("echo_i128", i128::MIN, i128);
    echo!("echo_u128", u128::MAX, u128);
    echo!("echo_i256", I256::MIN, I256);
    echo!("echo_u256", U256::MAX, U256);
    // No sequence advancement occurs for these simulations.
    assert_eq!(
        source.sequence_number(),
        server
            .get_account(&key.public_key())
            .await?
            .sequence_number()
    );
    Ok(())
}
