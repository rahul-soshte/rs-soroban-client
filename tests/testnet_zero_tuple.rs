//! Verify SDK Tuple([]) specs and raw Void values before native conversion.
#![cfg(feature = "contract-spec")]
mod support;

use soroban_client::{
    account::Account,
    contract::{ContractBehavior, Contracts},
    contract_client::Client,
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    soroban_rpc::SimulateTransactionResponse,
    transaction::{TransactionBuilder, TransactionBuilderBehavior},
    xdr::*,
    Options, Server,
};
use support::zero_tuple::*;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

async fn context() -> TestResult<(Server, Account, String)> {
    let id = std::env::var("ZERO_TUPLE_CONTRACT_ID")?;
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let key = Keypair::random()?;
    let source = server.request_airdrop(&key.public_key()).await?;
    Ok((server, source, id))
}

async fn raw_simulation(
    server: &Server,
    source: &Account,
    id: &str,
    function: &str,
    args: Vec<ScVal>,
) -> TestResult<SimulateTransactionResponse> {
    let contract = Contracts::new(id)?;
    let mut source = source.clone();
    let tx = TransactionBuilder::new(&mut source, Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(contract.call(function, Some(args)))
        .build();
    let simulation = server.simulate_transaction(&tx, None).await?;
    assert!(
        simulation.error.is_none(),
        "raw call {function} must succeed: {:?}",
        simulation.error
    );
    Ok(simulation)
}

#[tokio::test]
#[ignore = "requires ZERO_TUPLE_CONTRACT_ID from the updated native fixture"]
async fn deployed_zero_tuple_argument_and_result() -> TestResult<()> {
    let (server, source, id) = context().await?;
    let client = Client::new(&server, &id).await?;
    let function = client.spec().function("echo_unit")?;
    assert_eq!(function.inputs[0].type_, tuple_type());
    assert_eq!(function.outputs.as_slice(), &[tuple_type()]);
    let raw = raw_simulation(&server, &source, &id, "echo_unit", vec![ScVal::Void]).await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw result").0,
        ScVal::Void
    );
    let call = client
        .call("echo_unit", &((),))
        .expect("raw RPC accepted Void for Tuple([]), but native unit encoding failed");
    call.simulate::<()>(&source, Networks::testnet())
        .await
        .expect("raw RPC returned Void for Tuple([]), but native unit decoding failed");
    Ok(())
}

#[tokio::test]
#[ignore = "requires ZERO_TUPLE_CONTRACT_ID from the updated native fixture"]
async fn deployed_zero_tuple_result_without_unit_input() -> TestResult<()> {
    let (server, source, id) = context().await?;
    let client = Client::new(&server, &id).await?;
    let function = client.spec().function("unit_output")?;
    assert!(function.inputs.is_empty());
    assert_eq!(function.outputs.as_slice(), &[tuple_type()]);
    let raw = raw_simulation(&server, &source, &id, "unit_output", vec![]).await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw result").0,
        ScVal::Void
    );
    client
        .call("unit_output", &())?
        .simulate::<()>(&source, Networks::testnet())
        .await
        .expect("raw RPC returned Void for Tuple([]), but native unit decoding failed");
    Ok(())
}

#[tokio::test]
#[ignore = "requires ZERO_TUPLE_CONTRACT_ID from the updated native fixture"]
async fn deployed_zero_tuple_record_field() -> TestResult<()> {
    let (server, source, id) = context().await?;
    let client = Client::new(&server, &id).await?;
    let entry = client
        .spec()
        .entries()
        .iter()
        .find(|e| {
            matches!(e,
        ScSpecEntry::UdtStructV0(s) if s.name.to_utf8_string().unwrap() == "UnitRecord")
        })
        .expect("missing UnitRecord spec");
    let ScSpecEntry::UdtStructV0(record) = entry else {
        unreachable!()
    };
    assert_eq!(record.fields[0].type_, tuple_type());
    let raw = raw_simulation(&server, &source, &id, "unit_record", vec![]).await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw record").0,
        record_value()
    );
    let actual = client
        .call("unit_record", &())?
        .simulate::<UnitRecord>(&source, Networks::testnet())
        .await
        .expect("raw RPC returned a Void field for Tuple([]), but native record decoding failed");
    assert_eq!(actual, UnitRecord { value: () });
    Ok(())
}

#[tokio::test]
#[ignore = "requires ZERO_TUPLE_CONTRACT_ID from the updated native fixture"]
async fn deployed_zero_tuple_event_parameter() -> TestResult<()> {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct UnitDatum {
        value: (),
    }
    let (server, source, id) = context().await?;
    let client = Client::new(&server, &id).await?;
    let entry = client
        .spec()
        .entries()
        .iter()
        .find(|e| {
            matches!(e,
        ScSpecEntry::EventV0(s) if s.name.to_utf8_string().unwrap() == "UnitDatum")
        })
        .expect("missing UnitDatum spec");
    let ScSpecEntry::EventV0(event) = entry else {
        unreachable!()
    };
    assert_eq!(event.params[0].type_, tuple_type());
    let raw = raw_simulation(&server, &source, &id, "publish_unit", vec![ScVal::Void]).await?;
    let events: Vec<_> = raw
        .to_events()
        .expect("missing events")
        .into_iter()
        .filter(|e| e.event.type_ == ContractEventType::Contract)
        .collect();
    assert_eq!(events.len(), 1);
    assert!(events[0].in_successful_contract_call);
    let ContractEventBody::V0(body) = &events[0].event.body;
    assert_eq!(body.data, ScVal::Void);
    let actual: UnitDatum = client
        .decode_event_as("UnitDatum", &events[0])
        .expect("raw RPC emitted Void for Tuple([]), but native event decoding failed");
    assert_eq!(actual, UnitDatum { value: () });
    Ok(())
}
