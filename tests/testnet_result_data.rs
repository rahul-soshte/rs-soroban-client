//! Raw RPC behavior must succeed before native Result conversion is exercised.
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
use support::result_data::{self, Failure, ResultRecord};

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

async fn context() -> TestResult<(Server, Account, String)> {
    let id = std::env::var("RESULT_DATA_CONTRACT_ID")?;
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

macro_rules! input_case {
    ($name:ident, $function:literal, $value:expr, $wire:expr, $expected:expr) => {
        #[tokio::test]
        #[ignore = "requires RESULT_DATA_CONTRACT_ID from the updated native fixture"]
        async fn $name() -> TestResult<()> {
            let (server, source, id) = context().await?;
            let raw = raw_simulation(&server, &source, &id, $function, vec![$wire]).await?;
            assert_eq!(
                raw.try_result()?.expect("missing raw return value").0,
                ScVal::U32($expected)
            );
            let client = Client::new(&server, &id).await?;
            let call = client
                .call($function, &($value,))
                .expect("raw RPC accepted Result input, but native Result encoding failed");
            assert_eq!(
                call.simulate::<u32>(&source, Networks::testnet()).await?,
                $expected
            );
            Ok(())
        }
    };
}

input_case!(
    deployed_ok_input,
    "accept_result",
    Ok::<u32, Failure>(42),
    ScVal::U32(42),
    42
);
input_case!(
    deployed_err_input,
    "accept_result",
    Err::<u32, _>(Failure::Overflow),
    ScVal::Error(ScError::Contract(7)),
    7
);
input_case!(
    deployed_generic_err_input,
    "accept_any_result",
    Err::<u32, _>(ScError::Contract(99)),
    ScVal::Error(ScError::Contract(99)),
    99
);
input_case!(
    deployed_host_err_input,
    "accept_any_result",
    Err::<u32, _>(ScError::Storage(ScErrorCode::MissingValue)),
    ScVal::Error(ScError::Storage(ScErrorCode::MissingValue)),
    ScErrorCode::MissingValue as u32
);

#[tokio::test]
#[ignore = "requires RESULT_DATA_CONTRACT_ID from the updated native fixture"]
async fn deployed_nested_result_record() -> TestResult<()> {
    let (server, source, id) = context().await?;
    let raw = raw_simulation(&server, &source, &id, "result_record", vec![]).await?;
    assert_eq!(
        raw.try_result()?.expect("missing raw return record").0,
        result_data::record_value()
    );
    // Also establish that the SDK accepts this record and a mixed vector as inputs.
    let echoed = raw_simulation(
        &server,
        &source,
        &id,
        "echo_result_record",
        vec![result_data::record_value()],
    )
    .await?;
    assert_eq!(
        echoed.try_result()?.expect("missing echoed record").0,
        result_data::record_value()
    );
    let echoed = raw_simulation(
        &server,
        &source,
        &id,
        "echo_results",
        vec![result_data::vector()],
    )
    .await?;
    assert_eq!(
        echoed.try_result()?.expect("missing echoed vector").0,
        result_data::vector()
    );
    let client = Client::new(&server, &id).await?;
    let actual = client
        .call("result_record", &())?
        .try_simulate::<ResultRecord, Failure>(&source, Networks::testnet())
        .await
        .expect("raw RPC returned a successful record, but nested Result decoding failed");
    assert_eq!(actual, result_data::record());
    Ok(())
}

macro_rules! event_case {
    ($name:ident, $wire:expr, $expected:expr) => {
        #[tokio::test]
        #[ignore = "requires RESULT_DATA_CONTRACT_ID from the updated native fixture"]
        async fn $name() -> TestResult<()> {
            #[derive(Debug, PartialEq, serde::Deserialize)]
            struct ResultDatum {
                value: Result<u32, Failure>,
            }
            let (server, source, id) = context().await?;
            let wire = $wire;
            let raw =
                raw_simulation(&server, &source, &id, "publish_result", vec![wire.clone()]).await?;
            assert_eq!(
                raw.try_result()?.expect("missing raw return value").0,
                ScVal::Void
            );
            let events: Vec<_> = raw
                .to_events()
                .expect("missing events")
                .into_iter()
                .filter(|e| e.event.type_ == ContractEventType::Contract)
                .collect();
            assert_eq!(events.len(), 1);
            assert!(events[0].in_successful_contract_call);
            let ContractEventBody::V0(body) = &events[0].event.body;
            assert_eq!(body.data, wire);
            let client = Client::new(&server, &id).await?;
            let actual: ResultDatum = client.decode_event_as("ResultDatum", &events[0]).expect(
                "raw RPC emitted Result data successfully, but native event decoding failed",
            );
            assert_eq!(actual.value, $expected);
            Ok(())
        }
    };
}

event_case!(deployed_ok_event, ScVal::U32(42), Ok(42));
event_case!(
    deployed_err_event,
    ScVal::Error(ScError::Contract(7)),
    Err(Failure::Overflow)
);
