//! Red-phase tests for actual error-valued arguments, nested results and events.
//! Raw RPC assertions establish which values the contract accepts before the
//! currently missing native conversion is exercised.
#![cfg(feature = "contract-spec")]
mod support;

use serde::Deserialize;
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
use std::collections::BTreeMap;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, PartialEq, serde::Serialize, Deserialize)]
enum ArithmeticFailure {
    Overflow,
}

#[derive(Debug, PartialEq, Deserialize)]
struct ErrorRecord {
    typed: ArithmeticFailure,
    generic: ScError,
    values: Vec<ScError>,
    keyed: BTreeMap<u32, ScError>,
    optional: Option<ScError>,
}

async fn context() -> TestResult<(Server, Account, String)> {
    let id = std::env::var("ERROR_DATA_CONTRACT_ID")?;
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
        "raw contract call {function} must succeed: {:?}",
        simulation.error
    );
    Ok(simulation)
}

#[tokio::test]
#[ignore = "requires ERROR_DATA_CONTRACT_ID from the updated native fixture"]
async fn deployed_error_enum_argument_is_ordinary_input() -> TestResult<()> {
    let (server, source, id) = context().await?;
    let raw = raw_simulation(
        &server,
        &source,
        &id,
        "accept_error",
        vec![ScVal::Error(ScError::Contract(7))],
    )
    .await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw return value").0,
        ScVal::U32(7)
    );
    let client = Client::new(&server, &id).await?;
    let call = client
        .call("accept_error", &(ArithmeticFailure::Overflow,))
        .expect("raw RPC accepted the error argument, but native error enum encoding failed");
    assert_eq!(call.simulate::<u32>(&source, Networks::testnet()).await?, 7);
    Ok(())
}

#[tokio::test]
#[ignore = "requires ERROR_DATA_CONTRACT_ID from the updated native fixture"]
async fn deployed_error_record_is_successful_data() -> TestResult<()> {
    let (server, source, id) = context().await?;
    let raw = raw_simulation(&server, &source, &id, "error_record", Vec::new()).await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw error record").0,
        support::error_record_value()
    );
    let client = Client::new(&server, &id).await?;
    let actual = client
        .call("error_record", &())?
        .try_simulate::<ErrorRecord, ArithmeticFailure>(&source, Networks::testnet())
        .await
        .expect(
            "raw RPC returned a successful record, but native decoding of its error data failed",
        );
    assert_eq!(
        actual,
        ErrorRecord {
            typed: ArithmeticFailure::Overflow,
            generic: ScError::Contract(99),
            values: vec![ScError::Contract(99), ScError::Contract(7)],
            keyed: BTreeMap::from([(1, ScError::Contract(99))]),
            optional: Some(ScError::Contract(7)),
        }
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires ERROR_DATA_CONTRACT_ID from the updated native fixture"]
async fn deployed_error_event_preserves_error_type_and_code() -> TestResult<()> {
    #[derive(Debug, PartialEq, Deserialize)]
    struct ErrorDatum {
        value: ScError,
    }
    let (server, source, id) = context().await?;
    let host_error = ScError::Storage(ScErrorCode::MissingValue);
    let raw = raw_simulation(
        &server,
        &source,
        &id,
        "publish_errors",
        vec![
            ScVal::Error(ScError::Contract(7)),
            ScVal::Error(host_error.clone()),
        ],
    )
    .await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw result").0,
        ScVal::Void
    );
    let events: Vec<_> = raw
        .to_events()
        .expect("missing simulation events")
        .into_iter()
        .filter(|e| e.event.type_ == ContractEventType::Contract)
        .collect();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.in_successful_contract_call));
    let ContractEventBody::V0(typed) = &events[0].event.body;
    let ContractEventBody::V0(generic) = &events[1].event.body;
    assert_eq!(typed.data, ScVal::Error(ScError::Contract(7)));
    assert_eq!(generic.data, ScVal::Error(host_error.clone()));
    let client = Client::new(&server, &id).await?;
    let actual: ErrorDatum = client.decode_event_as("ErrorDatum", &events[1]).expect(
        "raw RPC emitted a successful error-valued event, but native event decoding failed",
    );
    assert_eq!(actual, ErrorDatum { value: host_error });
    Ok(())
}
