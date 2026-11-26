//! Red-phase tests for native Result inputs and nested data, not invocation errors.
#![cfg(feature = "contract-spec")]
mod support;

use soroban_client::{contract_spec::ContractSpec, xdr::*};
use std::collections::BTreeMap;
use support::{result_data::*, udt};

fn spec() -> ContractSpec {
    ContractSpec::new(support::entries()).unwrap()
}

// Separate encode/decode tests keep one unsupported direction from masking the other.
macro_rules! round_trip {
    ($encode:ident, $decode:ident, $rust:ty, $value:expr, $wire:expr, $ty:expr) => {
        #[test]
        fn $encode() {
            let value: $rust = $value;
            assert_eq!(
                spec()
                    .encode(&value, &$ty)
                    .expect("Result data should encode"),
                $wire
            );
        }
        #[test]
        fn $decode() {
            let expected: $rust = $value;
            let actual: $rust = spec()
                .decode(&$wire, &$ty)
                .expect("Result data should decode");
            assert_eq!(actual, expected);
        }
    };
}

round_trip!(encode_ok, decode_ok, Result<u32, Failure>, Ok(42), ScVal::U32(42), result_type());
round_trip!(encode_err, decode_err, Result<u32, Failure>, Err(Failure::Overflow), ScVal::Error(ScError::Contract(7)), result_type());
round_trip!(encode_generic_ok, decode_generic_ok, Result<u32, ScError>, Ok(42), ScVal::U32(42), result_with(ScSpecTypeDef::Error));
round_trip!(encode_generic_unknown_err, decode_generic_unknown_err, Result<u32, ScError>, Err(ScError::Contract(99)), ScVal::Error(ScError::Contract(99)), result_with(ScSpecTypeDef::Error));
round_trip!(encode_generic_host_err, decode_generic_host_err, Result<u32, ScError>, Err(ScError::Storage(ScErrorCode::MissingValue)), ScVal::Error(ScError::Storage(ScErrorCode::MissingValue)), result_with(ScSpecTypeDef::Error));
round_trip!(
    encode_vector,
    decode_vector,
    Vec<Result<u32, Failure>>,
    vec![Ok(42), Err(Failure::Overflow)],
    vector(),
    vec_type()
);
round_trip!(encode_map, decode_map, BTreeMap<u32, Result<u32, Failure>>, record().keyed, map(), map_type());
round_trip!(
    encode_some_ok,
    decode_some_ok,
    Option<Result<u32, Failure>>,
    Some(Ok(42)),
    ScVal::U32(42),
    option_type()
);
round_trip!(
    encode_some_err,
    decode_some_err,
    Option<Result<u32, Failure>>,
    Some(Err(Failure::Overflow)),
    ScVal::Error(ScError::Contract(7)),
    option_type()
);
round_trip!(
    encode_none,
    decode_none,
    Option<Result<u32, Failure>>,
    None,
    ScVal::Void,
    option_type()
);
round_trip!(
    encode_record,
    decode_record,
    ResultRecord,
    record(),
    record_value(),
    udt("ResultRecord")
);

#[test]
fn ok_result_argument() {
    assert_eq!(
        spec()
            .encode_args("accept_result", &(Ok::<u32, Failure>(42),))
            .expect("Ok input should encode as its unwrapped value"),
        vec![ScVal::U32(42)]
    );
}

#[test]
fn err_result_argument() {
    assert_eq!(
        spec()
            .encode_args("accept_result", &(Err::<u32, _>(Failure::Overflow),))
            .expect("Err input should encode as an error value"),
        vec![ScVal::Error(ScError::Contract(7))]
    );
}

#[test]
fn generic_result_argument() {
    assert_eq!(
        spec()
            .encode_args(
                "accept_any_result",
                &(Err::<u32, _>(ScError::Contract(99)),)
            )
            .expect("generic Err input should preserve undeclared codes"),
        vec![ScVal::Error(ScError::Contract(99))]
    );
}

#[test]
fn nested_result_function_output() {
    assert_eq!(
        spec()
            .decode_result::<ResultRecord>("result_record", &record_value())
            .expect("errors inside a successful function result must remain data"),
        record()
    );
}

#[test]
fn invalid_result_payloads_are_rejected() {
    let spec = spec();
    for raw in [
        ScVal::Error(ScError::Contract(99)),
        ScVal::Error(ScError::Storage(ScErrorCode::MissingValue)),
        ScVal::Bool(true),
    ] {
        assert!(spec
            .decode::<Result<u32, Failure>>(&raw, &result_type())
            .is_err());
    }
}

#[test]
fn malformed_ok_and_unknown_err_variants_are_rejected() {
    #[derive(serde::Serialize)]
    enum OtherFailure {
        Unknown,
    }
    assert!(spec()
        .encode(&Ok::<_, Failure>("not a u32"), &result_type())
        .is_err());
    assert!(spec()
        .encode(&Err::<u32, _>(OtherFailure::Unknown), &result_type())
        .is_err());
}

fn event(prefix: &str, data: ScVal) -> ContractEvent {
    ContractEvent {
        contract_id: Some(ContractId(Hash([0; 32]))),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: vec![ScVal::Symbol(prefix.try_into().unwrap())]
                .try_into()
                .unwrap(),
            data,
        }),
        ..Default::default()
    }
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct ResultDatum {
    value: Result<u32, Failure>,
}

#[test]
fn ok_event_parameter() {
    let actual: ResultDatum = spec()
        .decode_event_as("ResultDatum", &event("result_datum", ScVal::U32(42)))
        .expect("non-error event data should select Ok");
    assert_eq!(actual.value, Ok(42));
}

#[test]
fn err_event_parameter() {
    let actual: ResultDatum = spec()
        .decode_event_as(
            "ResultDatum",
            &event("result_datum", ScVal::Error(ScError::Contract(7))),
        )
        .expect("error event data should select Err, not invocation failure");
    assert_eq!(actual.value, Err(Failure::Overflow));
}

#[test]
fn record_event_parameter() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct ResultReport {
        value: ResultRecord,
    }
    let actual: ResultReport = spec()
        .decode_event_as("ResultReport", &event("result_report", record_value()))
        .expect("nested Result event fields should decode");
    assert_eq!(actual.value, record());
}

// Result is untagged on the wire: Ok(error) and Ok(Err(...)) cannot be
// distinguished from Err(...). No lossless round trip is promised for these.
