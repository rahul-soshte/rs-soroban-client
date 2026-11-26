#![cfg(feature = "contract-spec")]
mod support;

use serde_json::json;
use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    contract_client::{CallError, Client},
    contract_spec::{ContractSpec, EventDecodeError},
    xdr::*,
    Options, Server,
};
use support::*;
use wiremock::{matchers::body_partial_json, Mock, MockServer, ResponseTemplate};

async fn rpc(simulation: serde_json::Value) -> (Server, MockServer) {
    let mock = MockServer::start().await;
    let address = Address::new(CONTRACT).unwrap().to_sc_address().unwrap();
    let instance = LedgerEntryData::ContractData(ContractDataEntry {
        contract: address,
        key: ScVal::LedgerKeyContractInstance,
        val: ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::Wasm(Hash([1; 32])),
            storage: None,
        }),
        ..Default::default()
    });
    let code = LedgerEntryData::ContractCode(ContractCodeEntry {
        hash: Hash([1; 32]),
        code: interface_wasm().try_into().unwrap(),
        ..Default::default()
    });
    Mock::given(body_partial_json(json!({"method":"getLedgerEntries"})))
        .respond_with(move |request: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let key = &body["params"]["keys"][0];
            let parsed = LedgerKey::from_xdr_base64(key.as_str().unwrap(),Limits::none()).unwrap();
            let data = if matches!(parsed,LedgerKey::ContractCode(_)) { &code } else { &instance };
            ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":1,"result":{
                "latestLedger":100, "entries":[{"key":key,"xdr":data.to_xdr_base64(Limits::none()).unwrap(),"lastModifiedLedgerSeq":99}]
            }}))
        }).mount(&mock).await;
    Mock::given(body_partial_json(json!({"method":"simulateTransaction"})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc":"2.0","id":1,"result":simulation})),
        )
        .mount(&mock)
        .await;
    let server = Server::new(
        &mock.uri(),
        Options {
            allow_http: true,
            ..Default::default()
        },
    )
    .unwrap();
    (server, mock)
}

fn success() -> serde_json::Value {
    json!({"latestLedger":100,"minResourceFee":"1", "transactionData":SorobanTransactionData::default().to_xdr_base64(Limits::none()).unwrap(),
        "results":[{"auth":[],"xdr":ScVal::U32(5).to_xdr_base64(Limits::none()).unwrap()}]})
}

fn diagnostic(contract: u8, error: ScError) -> String {
    DiagnosticEvent {
        in_successful_contract_call: false,
        event: ContractEvent {
            contract_id: Some(ContractId(Hash([contract; 32]))),
            type_: ContractEventType::Diagnostic,
            body: ContractEventBody::V0(ContractEventV0 {
                topics: vec![
                    ScVal::Symbol("error".try_into().unwrap()),
                    ScVal::Error(error),
                ]
                .try_into()
                .unwrap(),
                data: ScVal::Void,
            }),
            ..Default::default()
        },
    }
    .to_xdr_base64(Limits::none())
    .unwrap()
}

#[tokio::test]
async fn discovery_native_simulation_and_single_simulation_preparation() {
    let (server, mock) = rpc(success()).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    assert_eq!(client.sep_claims().unwrap(), vec![41, 40, 48]);
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client.call("add", &(2u32, 3u32)).unwrap();
    assert_eq!(call.simulate::<u32>(&source, "test").await.unwrap(), 5);
    let prepared = call.prepare::<u32>(&source, "test").await.unwrap();
    assert_eq!(prepared.simulated_value, 5);
    assert_eq!(prepared.transaction.fee, 1001);
    let requests = mock.received_requests().await.unwrap();
    let count = |method: &str| {
        requests
            .iter()
            .filter(|r| {
                serde_json::from_slice::<serde_json::Value>(&r.body).unwrap()["method"] == method
            })
            .count()
    };
    assert_eq!(
        count("getLedgerEntries"),
        2,
        "client should fetch the Wasm only once"
    );
    assert_eq!(
        count("simulateTransaction"),
        2,
        "prepare must not re-simulate"
    );
    assert_eq!(
        server.get_contract_spec(CONTRACT).await.unwrap().entries(),
        entries()
    );
    assert_eq!(
        server
            .get_contract_metadata(CONTRACT)
            .await
            .unwrap()
            .entries
            .len(),
        2
    );
    assert_eq!(
        server.get_contract_sep_claims(CONTRACT).await.unwrap(),
        vec![41, 40, 48]
    );
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum ArithmeticError {
    Overflow,
}

#[tokio::test]
async fn result_data_ok_argument_is_unwrapped_on_the_wire() {
    use support::result_data::Failure;
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(ScVal::U32(42).to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client
        .call("accept_result", &(Ok::<u32, Failure>(42),))
        .expect("Result::Ok input should encode without a Serde wrapper");
    let OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
        host_function: HostFunction::InvokeContract(args),
        ..
    }) = call.operation().body
    else {
        panic!("expected contract invocation");
    };
    assert_eq!(args.args.as_slice(), &[ScVal::U32(42)]);
    assert_eq!(call.simulate::<u32>(&source, "test").await.unwrap(), 42);
}

#[tokio::test]
async fn result_data_err_argument_is_not_an_invocation_failure() {
    use support::result_data::Failure;
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(ScVal::U32(7).to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client
        .call("accept_result", &(Err::<u32, _>(Failure::Overflow),))
        .expect("Result::Err input should encode as ordinary error-valued input");
    assert_eq!(call.simulate::<u32>(&source, "test").await.unwrap(), 7);
}

#[tokio::test]
async fn result_data_nested_err_remains_a_successful_result() {
    use support::result_data::{self, ResultRecord};
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(result_data::record_value()
        .to_xdr_base64(Limits::none())
        .unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let actual = client
        .call("result_record", &())
        .unwrap()
        .try_simulate::<ResultRecord, ArithmeticError>(&source, "test")
        .await
        .expect("nested Err values should decode inside a successful function result");
    assert_eq!(actual, result_data::record());
}

#[tokio::test]
async fn error_data_contract_enum_is_an_ordinary_argument() {
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(ScVal::U32(7).to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client
        .call("accept_error", &(ArithmeticError::Overflow,))
        .expect("a declared error enum should be usable as an ordinary argument");
    assert_eq!(call.simulate::<u32>(&source, "test").await.unwrap(), 7);
}

#[tokio::test]
async fn error_data_nested_values_do_not_become_invocation_failures() {
    let errors = vec![
        ScError::Contract(7),
        ScError::Storage(ScErrorCode::MissingValue),
    ];
    let raw_values: Vec<_> = errors.iter().cloned().map(ScVal::Error).collect();
    let raw = ScVal::Vec(Some(raw_values.clone().try_into().unwrap()));
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(raw.to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let actual = client
        .call("echo_errors", &(&raw_values,))
        .expect("the existing raw ScVal error argument escape hatch must remain usable")
        .try_simulate::<Vec<ScError>, ArithmeticError>(&source, "test")
        .await
        .expect(
            "errors inside a successful result vector must decode as data, not CallError::Contract",
        );
    assert_eq!(actual, errors);
}

#[tokio::test]
async fn error_data_record_is_a_successful_function_result() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct ErrorRecord {
        typed: ArithmeticError,
        generic: ScError,
        values: Vec<ScError>,
        keyed: std::collections::BTreeMap<u32, ScError>,
        optional: Option<ScError>,
    }
    let mut simulation = success();
    simulation["results"][0]["xdr"] =
        json!(error_record_value().to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let actual = client
        .call("error_record", &())
        .unwrap()
        .try_simulate::<ErrorRecord, ArithmeticError>(&source, "test")
        .await
        .expect("a successful record containing error values must remain a successful result");
    assert_eq!(
        actual,
        ErrorRecord {
            typed: ArithmeticError::Overflow,
            generic: ScError::Contract(99),
            values: vec![ScError::Contract(99), ScError::Contract(7)],
            keyed: std::collections::BTreeMap::from([(1, ScError::Contract(99))]),
            optional: Some(ScError::Contract(7)),
        }
    );
}

#[tokio::test]
async fn native_composite_maps_through_client() {
    use std::collections::{BTreeMap, HashMap};
    let values = BTreeMap::from([((1u32, 2u32), i128::MIN), ((2, 1), i128::MAX)]);
    let spec = ContractSpec::new(entries()).unwrap();
    let wire = spec.encode(&values, &map_type()).unwrap();
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(wire.to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client.call("echo_map", &(&values,)).unwrap();
    let result: HashMap<(u32, u32), i128> = call.simulate(&source, "test").await.unwrap();
    assert_eq!(result, values.clone().into_iter().collect());
    assert_eq!(
        call.prepare::<BTreeMap<(u32, u32), i128>>(&source, "test")
            .await
            .unwrap()
            .simulated_value,
        values
    );
}

#[tokio::test]
async fn generic_val_simulation_returns_explicit_scval() {
    let expected = generic_val_value();
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(expected.to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client
        .call("echo_val", &(&expected,))
        .expect("explicit ScVal arguments should already encode for generic Val");
    let OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
        host_function: HostFunction::InvokeContract(args),
        ..
    }) = call.operation().body
    else {
        panic!("expected a contract invocation");
    };
    assert_eq!(args.args.as_slice(), std::slice::from_ref(&expected));
    let actual: ScVal = call
        .simulate(&source, "test")
        .await
        .expect("a generic Val simulation result should retain its explicit ScVal");
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn generic_val_preparation_preserves_the_simulated_scval() {
    let expected = generic_val_value();
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(expected.to_xdr_base64(Limits::none()).unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let prepared = client
        .call("echo_val", &(&expected,))
        .unwrap()
        .prepare::<ScVal>(&source, "test")
        .await
        .expect("preparation should retain a generic Val result as ScVal");
    assert_eq!(prepared.simulated_value, expected);
}

#[tokio::test]
async fn generic_val_root_contract_error_still_uses_invocation_error_boundary() {
    let mut simulation = success();
    simulation["results"][0]["xdr"] = json!(ScVal::Error(ScError::Contract(7))
        .to_xdr_base64(Limits::none())
        .unwrap());
    let (server, _mock) = rpc(simulation).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let value = ScVal::U32(42);
    let result = client
        .call("echo_val", &(&value,))
        .unwrap()
        .try_simulate::<ScVal, ArithmeticError>(&source, "test")
        .await;
    assert!(matches!(
        result,
        Err(CallError::Contract(ArithmeticError::Overflow))
    ));
}

#[tokio::test]
async fn typed_events_validate_the_emitting_contract() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct Added {
        value: u32,
    }
    let (server, _mock) = rpc(success()).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let event = ContractEvent {
        contract_id: Some(ContractId(Hash([0; 32]))),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: vec![ScVal::Symbol("added".try_into().unwrap())]
                .try_into()
                .unwrap(),
            data: ScVal::U32(5),
        }),
        ..Default::default()
    };
    assert_eq!(
        client.decode_event::<Added>(&event).unwrap(),
        Added { value: 5 }
    );
    assert_eq!(
        client.decode_event_as::<Added>("Added", &event).unwrap(),
        Added { value: 5 }
    );
    let rpc_event:soroban_client::soroban_rpc::EventResponse = serde_json::from_value(json!({
        "type":"contract","contractId":CONTRACT,"ledger":100,"ledgerClosedAt":"today","id":"1-1","txHash":"test",
        "topic":[ScVal::Symbol("added".try_into().unwrap()).to_xdr_base64(Limits::none()).unwrap()],
        "value":ScVal::U32(5).to_xdr_base64(Limits::none()).unwrap(),
    })).unwrap();
    assert_eq!(
        client.decode_event::<Added>(&rpc_event).unwrap(),
        Added { value: 5 }
    );
    let other_event = ContractEvent {
        contract_id: Some(ContractId(Hash([1; 32]))),
        ..event.clone()
    };
    assert!(matches!(
        client.decode_event::<Added>(&other_event),
        Err(EventDecodeError::WrongContract { .. })
    ));
    assert!(matches!(
        client.decode_event_as::<Added>("Added", &other_event),
        Err(EventDecodeError::WrongContract { .. })
    ));
    let mut other_rpc = rpc_event;
    other_rpc.contract_id =
        Address::from_sc_address(&ScAddress::Contract(ContractId(Hash([1; 32]))))
            .unwrap()
            .to_string();
    assert!(matches!(
        client.decode_event::<Added>(&other_rpc),
        Err(EventDecodeError::WrongContract { .. })
    ));
    // Offline spec decoding intentionally does not bind the emitter to an ID.
    assert_eq!(
        client.spec().decode_event::<Added>(&other_event).unwrap(),
        Added { value: 5 }
    );
}

#[tokio::test]
async fn structured_contract_error_maps_to_native_variant() {
    let (server, _mock) = rpc(json!({"latestLedger":100,"error":"opaque failure text",
        "events":[diagnostic(0,ScError::Contract(7))]}))
    .await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    let source = Account::new(ACCOUNT, "0").unwrap();
    let call = client.call("add", &(2u32, 3u32)).unwrap();
    assert!(matches!(
        call.try_simulate::<u32, ArithmeticError>(&source, "test")
            .await,
        Err(CallError::Contract(ArithmeticError::Overflow))
    ));
    match call.simulate::<u32>(&source, "test").await {
        Err(CallError::Contract(error)) => {
            assert_eq!(error.code, 7);
            assert_eq!(error.candidates[0].enum_name, "ArithmeticFailure");
        }
        result => panic!("unexpected {result:?}"),
    }
}

#[tokio::test]
async fn unknown_codes_and_nested_errors_are_not_misidentified() {
    let source = Account::new(ACCOUNT, "0").unwrap();
    let (server,_mock) = rpc(json!({"latestLedger":100,"error":"failure","events":[diagnostic(0,ScError::Contract(99))]})).await;
    let client = Client::new(&server, CONTRACT).await.unwrap();
    assert!(
        matches!(client.call("add",&(1,2)).unwrap().try_simulate::<u32,ArithmeticError>(&source,"test").await,Err(CallError::UnknownContract(e)) if e.code == 99)
    );
    for events in [
        vec![diagnostic(1, ScError::Contract(7))],
        vec![
            diagnostic(0, ScError::Contract(7)),
            diagnostic(0, ScError::WasmVm(ScErrorCode::InvalidAction)),
        ],
    ] {
        let (server, _mock) =
            rpc(json!({"latestLedger":100,"error":"failure","events":events})).await;
        let client = Client::new(&server, CONTRACT).await.unwrap();
        assert!(matches!(
            client
                .call("add", &(1, 2))
                .unwrap()
                .try_simulate::<u32, ArithmeticError>(&source, "test")
                .await,
            Err(CallError::Simulation { .. })
        ));
    }
}

#[tokio::test]
async fn malformed_rpc_results_are_fallible() {
    let source = Account::new(ACCOUNT, "0").unwrap();
    for results in [json!([]), json!([{"auth":[],"xdr":"bad"}])] {
        let (server, _mock) = rpc(json!({"latestLedger":100,"results":results})).await;
        let client = Client::new(&server, CONTRACT).await.unwrap();
        assert!(client
            .call("add", &(1, 2))
            .unwrap()
            .simulate::<u32>(&source, "test")
            .await
            .is_err());
    }
    for data in [serde_json::Value::Null, json!("bad")] {
        let mut simulation = success();
        simulation["transactionData"] = data;
        let (server, _mock) = rpc(simulation).await;
        let client = Client::new(&server, CONTRACT).await.unwrap();
        assert!(client
            .call("add", &(1, 2))
            .unwrap()
            .prepare::<u32>(&source, "test")
            .await
            .is_err());
    }
}

#[test]
fn error_resolution_handles_ambiguity_and_explicit_udts() {
    let mut spec_entries = entries();
    let mut other = match spec_entries[5].clone() {
        ScSpecEntry::UdtErrorEnumV0(e) => e,
        _ => panic!(),
    };
    other.name = "OtherFailure".try_into().unwrap();
    spec_entries.push(ScSpecEntry::UdtErrorEnumV0(other));
    let spec = ContractSpec::new(spec_entries.clone()).unwrap();
    assert_eq!(spec.contract_error("add", 7).candidates.len(), 2);
    spec_entries.push(function(
        "explicit",
        &[],
        ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
            ok_type: Box::new(ScSpecTypeDef::U32),
            error_type: Box::new(udt("OtherFailure")),
        })),
    ));
    let spec = ContractSpec::new(spec_entries).unwrap();
    assert_eq!(
        spec.contract_error("explicit", 7).candidates[0].enum_name,
        "OtherFailure"
    );
    assert_eq!(spec.contract_error("explicit", 7).candidates.len(), 1);
}
