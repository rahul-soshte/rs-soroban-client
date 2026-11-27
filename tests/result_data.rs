//! Native Result inputs and nested data, distinct from invocation errors.
#![cfg(feature = "contract-spec")]
mod support;

use soroban_client::{contract_spec::ContractSpec, xdr::*};
use std::collections::BTreeMap;
use support::{result_data::*, udt};

fn spec() -> ContractSpec {
    ContractSpec::new(support::entries()).unwrap()
}

// Test each conversion direction separately.
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

#[test]
fn result_ok_payload_preserves_composite_maps() {
    let spec = spec();
    let ty = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(support::map_type()),
        error_type: Box::new(support::udt("ArithmeticFailure")),
    }));
    let values = BTreeMap::from([((1u32, 2u32), i128::MIN), ((2, 1), i128::MAX)]);
    let raw = spec.encode(&values, &support::map_type()).unwrap();
    let value: Result<_, Failure> = Ok(values);
    assert_eq!(spec.encode(&value, &ty).unwrap(), raw);
    assert_eq!(
        spec.decode::<Result<BTreeMap<(u32, u32), i128>, Failure>>(&raw, &ty)
            .unwrap(),
        value
    );
}

#[test]
fn result_ok_unit_and_optional_payloads_preserve_void() {
    let spec = spec();
    let unit = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(ScSpecTypeDef::Void),
        error_type: Box::new(ScSpecTypeDef::Error),
    }));
    assert_eq!(
        spec.encode(&Ok::<(), ScError>(()), &unit).unwrap(),
        ScVal::Void
    );
    assert_eq!(
        spec.decode::<Result<(), ScError>>(&ScVal::Void, &unit)
            .unwrap(),
        Ok(())
    );
    let optional = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(ScSpecTypeDef::U32),
        }))),
        error_type: Box::new(ScSpecTypeDef::Error),
    }));
    assert_eq!(
        spec.encode(&Ok::<Option<u32>, ScError>(None), &optional)
            .unwrap(),
        ScVal::Void
    );
    assert_eq!(
        spec.decode::<Result<Option<u32>, ScError>>(&ScVal::Void, &optional)
            .unwrap(),
        Ok(None)
    );
}

#[test]
fn result_nested_directly_in_ok_handles_unambiguous_values() {
    let ty = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(result_type()),
        error_type: Box::new(ScSpecTypeDef::Error),
    }));
    let spec = spec();
    for (value, raw) in [
        (Ok(Ok(42)), ScVal::U32(42)),
        (
            Err(ScError::Contract(99)),
            ScVal::Error(ScError::Contract(99)),
        ),
    ] {
        assert_eq!(spec.encode(&value, &ty).unwrap(), raw);
        assert_eq!(
            spec.decode::<Result<Result<u32, Failure>, ScError>>(&raw, &ty)
                .unwrap(),
            value
        );
    }
}

#[test]
fn result_rejects_malformed_wrappers_and_nonerror_err_payloads() {
    let spec = spec();
    for value in [
        serde_json::json!({"Neither": 42}),
        serde_json::json!({"Ok": 42, "Err": "Overflow"}),
        serde_json::json!("Ok"),
        serde_json::json!({"Err": 7}),
    ] {
        assert!(spec.encode(&value, &result_type()).is_err());
    }
    let invalid = result_with(ScSpecTypeDef::U32);
    assert!(spec.encode(&Err::<u32, u32>(7), &invalid).is_err());
}

#[test]
fn result_ok_rejects_direct_error_payloads() {
    let spec = spec();
    for ok_type in [
        ScSpecTypeDef::Error,
        ScSpecTypeDef::Val,
        support::udt("ArithmeticFailure"),
    ] {
        let ty = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
            ok_type: Box::new(ok_type.clone()),
            error_type: Box::new(ScSpecTypeDef::Error),
        }));
        let rejected = match ok_type {
            ScSpecTypeDef::Error => spec.encode(&Ok::<_, ScError>(ScError::Contract(7)), &ty),
            ScSpecTypeDef::Val => {
                spec.encode(&Ok::<_, ScError>(ScVal::Error(ScError::Contract(7))), &ty)
            }
            _ => spec.encode(&Ok::<_, ScError>(Failure::Overflow), &ty),
        };
        assert!(rejected.is_err(), "Ok must not encode as SCV_ERROR");
    }
}

#[test]
fn result_ok_rejects_nested_err_without_a_container() {
    let ty = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(result_type()),
        error_type: Box::new(ScSpecTypeDef::Error),
    }));
    assert!(spec()
        .encode(
            &Ok::<Result<u32, Failure>, ScError>(Err(Failure::Overflow)),
            &ty
        )
        .is_err());
}

#[test]
fn result_ok_allows_errors_inside_a_container() {
    let ty = ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(vec_type()),
        error_type: Box::new(ScSpecTypeDef::Error),
    }));
    let value = Ok::<_, ScError>(vec![Ok(42), Err(Failure::Overflow)]);
    let spec = spec();
    assert_eq!(spec.encode(&value, &ty).unwrap(), vector());
    assert_eq!(
        spec.decode::<Result<Vec<Result<u32, Failure>>, ScError>>(&vector(), &ty)
            .unwrap(),
        value
    );
}
