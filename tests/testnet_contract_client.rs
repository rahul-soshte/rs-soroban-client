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
    soroban_rpc::{EventType, GetTransactionResponse, TransactionStatus},
    transaction::{
        Transaction, TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior,
    },
    EventFilter, Options, Pagination, Server,
};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    z: i128,
    a: String,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct MapKey {
    z: u32,
    a: u32,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct MapContainer {
    values: BTreeMap<MapKey, Vec<Option<u128>>>,
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

#[derive(Debug, PartialEq, Deserialize)]
struct Added {
    value: u32,
}
#[derive(Debug, PartialEq, Deserialize)]
struct NativeCount {
    owner: String,
    value: u128,
}
#[derive(Debug, PartialEq, Deserialize)]
struct NativeText {
    owner: String,
    text: String,
}
#[derive(Debug, PartialEq, Deserialize)]
struct NativeValues {
    owner: String,
    count: u128,
    values: BTreeMap<(u32, u32), i128>,
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
    let (_, rows) = confirmed.to_events().ok_or("missing confirmed events")?;
    let events: Vec<_> = rows.iter().flatten().collect();
    assert_eq!(events.len(), 1);
    assert_eq!(client.decode_event::<Added>(events[0])?, Added { value: 5 });
    let response = server
        .get_events(
            Pagination::From(confirmed.ledger.ok_or("missing ledger")?),
            vec![EventFilter::new(EventType::Contract).contract(&id)],
            100,
        )
        .await?;
    assert_eq!(response.events.len(), 1);
    assert_eq!(
        client.decode_event::<Added>(&response.events[0])?,
        Added { value: 5 }
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
    let values = BTreeMap::from([((2u32, 1u32), i128::MIN), ((1, 2), i128::MAX)]);
    let call = client.call("echo_map", &(&values,))?;
    let prepared = call
        .prepare::<BTreeMap<(u32, u32), i128>>(&source, Networks::testnet())
        .await?;
    assert_eq!(prepared.simulated_value, values);
    let confirmed = submit(&server, prepared.transaction, &key).await?;
    assert_eq!(
        call.decode_transaction::<HashMap<(u32, u32), i128>, ArithmeticFailure>(&confirmed)?,
        values.into_iter().collect()
    );
    source = server.get_account(&key.public_key()).await?;
    echo!("echo_struct_map",HashMap::from([(MapKey { z:0,a:9 },U256::MAX),(MapKey { z:1,a:0 },U256::ZERO)]),HashMap<MapKey,U256>);
    echo!(
        "echo_nested_map",
        MapContainer {
            values: BTreeMap::from([(MapKey { z: 7, a: 3 }, vec![None, Some(u128::MAX)])])
        },
        MapContainer
    );
    // No sequence advancement occurs for these simulations.
    assert_eq!(
        source.sequence_number(),
        server
            .get_account(&key.public_key())
            .await?
            .sequence_number()
    );
    let values = BTreeMap::from([((1u32, 2u32), i128::MIN), ((2, 1), i128::MAX)]);
    let call = client.call("emit_events", &(u128::MAX, &values))?;
    let prepared = call.prepare::<()>(&source, Networks::testnet()).await?;
    let confirmed = submit(&server, prepared.transaction, &key).await?;
    call.decode_transaction::<(), ArithmeticFailure>(&confirmed)?;
    let (_, rows) = confirmed.to_events().ok_or("missing native events")?;
    let events: Vec<_> = rows.iter().flatten().collect();
    assert_eq!(events.len(), 3);
    assert_eq!(
        client.decode_event::<NativeCount>(events[0])?,
        NativeCount {
            owner: id.clone(),
            value: u128::MAX
        }
    );
    assert_eq!(
        client.decode_event::<NativeText>(events[1])?,
        NativeText {
            owner: id.clone(),
            text: "hello".into()
        }
    );
    assert_eq!(
        client.decode_event::<NativeValues>(events[2])?,
        NativeValues {
            owner: id.clone(),
            count: u128::MAX,
            values: values.clone()
        }
    );
    assert_eq!(
        client.decode_event_as::<NativeCount>("NativeCount", events[0])?,
        NativeCount {
            owner: id.clone(),
            value: u128::MAX
        }
    );
    let response = server
        .get_events(
            Pagination::From(confirmed.ledger.ok_or("missing event ledger")?),
            vec![EventFilter::new(EventType::Contract).contract(&id)],
            100,
        )
        .await?;
    assert_eq!(response.events.len(), 3);
    assert_eq!(
        client
            .decode_event::<NativeCount>(&response.events[0])?
            .value,
        u128::MAX
    );
    assert_eq!(
        client.decode_event::<NativeText>(&response.events[1])?.text,
        "hello"
    );
    assert_eq!(
        client
            .decode_event::<NativeValues>(&response.events[2])?
            .values,
        values
    );
    Ok(())
}
