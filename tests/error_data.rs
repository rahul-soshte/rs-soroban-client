//! Errors used as ordinary values, not as top-level invocation failures.
//! Generic ScError and declared contract-error enums retain their wire identity.
#![cfg(feature = "contract-spec")]
mod support;

use serde::{Deserialize, Serialize};
use soroban_client::{
    contract_spec::{ContractSpec, EventDecodeError},
    xdr::*,
};
use std::collections::BTreeMap;
use support::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ArithmeticFailure {
    Overflow,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum RenamedFailure {
    #[serde(rename = "Overflow")]
    TooLarge,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ErrorRecord {
    typed: ArithmeticFailure,
    generic: ScError,
    values: Vec<ScError>,
    keyed: BTreeMap<u32, ScError>,
    optional: Option<ScError>,
}

fn record() -> ErrorRecord {
    ErrorRecord {
        typed: ArithmeticFailure::Overflow,
        generic: ScError::Contract(99),
        values: vec![ScError::Contract(99), ScError::Contract(7)],
        keyed: BTreeMap::from([(1, ScError::Contract(99))]),
        optional: Some(ScError::Contract(7)),
    }
}

fn spec() -> ContractSpec {
    ContractSpec::new(entries()).unwrap()
}
fn vector(values: Vec<ScVal>) -> ScVal {
    ScVal::Vec(Some(values.try_into().unwrap()))
}

macro_rules! generic_error_case {
    ($encode:ident, $decode:ident, $error:expr) => {
        #[test]
        fn $encode() {
            let value = $error;
            assert_eq!(
                spec()
                    .encode(&value, &ScSpecTypeDef::Error)
                    .expect("generic Error encoding should preserve its type and code"),
                ScVal::Error(value)
            );
        }
        #[test]
        fn $decode() {
            let expected = $error;
            let actual: ScError = spec()
                .decode(&ScVal::Error(expected.clone()), &ScSpecTypeDef::Error)
                .expect("generic Error decoding should preserve its type and code");
            assert_eq!(actual, expected);
        }
    };
}
generic_error_case!(
    encode_known_contract_error,
    decode_known_contract_error,
    ScError::Contract(7)
);
generic_error_case!(
    encode_unknown_contract_error,
    decode_unknown_contract_error,
    ScError::Contract(99)
);
generic_error_case!(
    encode_max_contract_code,
    decode_max_contract_code,
    ScError::Contract(u32::MAX)
);
generic_error_case!(
    encode_storage_error,
    decode_storage_error,
    ScError::Storage(ScErrorCode::MissingValue)
);
generic_error_case!(
    encode_auth_error,
    decode_auth_error,
    ScError::Auth(ScErrorCode::InvalidAction)
);
generic_error_case!(
    encode_vm_error,
    decode_vm_error,
    ScError::WasmVm(ScErrorCode::InternalError)
);

#[test]
fn typed_error_enum_encodes_as_error_not_u32() {
    let value = ArithmeticFailure::Overflow;
    let actual = spec()
        .encode(&value, &udt("ArithmeticFailure"))
        .expect("declared error enum variants should encode as contract errors");
    assert_eq!(actual, ScVal::Error(ScError::Contract(7)));
}

#[test]
fn typed_error_enum_decodes_to_native_variant() {
    let actual: ArithmeticFailure = spec()
        .decode(
            &ScVal::Error(ScError::Contract(7)),
            &udt("ArithmeticFailure"),
        )
        .expect("a declared error code should decode as its Rust error variant");
    assert_eq!(actual, ArithmeticFailure::Overflow);
}

#[test]
fn error_enum_encoding_respects_serde_variant_names() {
    assert_eq!(
        spec()
            .encode(&RenamedFailure::TooLarge, &udt("ArithmeticFailure"))
            .expect("Serde rename should identify the declared error variant"),
        ScVal::Error(ScError::Contract(7))
    );
}

#[test]
fn error_enum_decoding_respects_serde_variant_names() {
    assert_eq!(
        spec()
            .decode::<RenamedFailure>(
                &ScVal::Error(ScError::Contract(7)),
                &udt("ArithmeticFailure")
            )
            .expect("Serde rename should map the declared variant into Rust"),
        RenamedFailure::TooLarge
    );
}

#[test]
fn unknown_codes_are_rejected_for_typed_enums() {
    assert!(spec()
        .decode::<ArithmeticFailure>(
            &ScVal::Error(ScError::Contract(99)),
            &udt("ArithmeticFailure")
        )
        .is_err());
}

#[test]
fn noncontract_errors_and_u32_are_not_typed_error_variants() {
    let spec = spec();
    assert!(spec
        .decode::<ArithmeticFailure>(
            &ScVal::Error(ScError::Storage(ScErrorCode::MissingValue)),
            &udt("ArithmeticFailure")
        )
        .is_err());
    assert!(spec
        .decode::<ArithmeticFailure>(&ScVal::U32(7), &udt("ArithmeticFailure"))
        .is_err());
    assert!(spec.encode(&7u32, &udt("ArithmeticFailure")).is_err());
    assert!(spec
        .decode::<ScError>(&ScVal::U32(7), &ScSpecTypeDef::Error)
        .is_err());
}

#[test]
fn typed_error_mapping_uses_the_referenced_enum_not_global_code_lookup() {
    let mut entries = entries();
    entries.push(ScSpecEntry::UdtErrorEnumV0(ScSpecUdtErrorEnumV0 {
        name: "OtherFailure".try_into().unwrap(),
        cases: vec![ScSpecUdtErrorEnumCaseV0 {
            name: "DifferentMeaning".try_into().unwrap(),
            value: 7,
            doc: Default::default(),
        }]
        .try_into()
        .unwrap(),
        ..Default::default()
    }));
    let spec = ContractSpec::new(entries).unwrap();
    assert_eq!(
        spec.decode::<ArithmeticFailure>(
            &ScVal::Error(ScError::Contract(7)),
            &udt("ArithmeticFailure")
        )
        .expect("another enum's identical numeric code must not make a typed field ambiguous"),
        ArithmeticFailure::Overflow
    );
}

#[test]
fn typed_error_argument_uses_its_declared_error_type() {
    assert_eq!(
        spec()
            .encode_args("accept_error", &(ArithmeticFailure::Overflow,))
            .expect("contract error enum arguments should be ordinary input values"),
        vec![ScVal::Error(ScError::Contract(7))]
    );
}

#[test]
fn generic_error_argument_does_not_require_a_declared_contract_case() {
    assert_eq!(
        spec()
            .encode_args("accept_generic_error", &(ScError::Contract(99),))
            .expect("generic Error arguments should preserve undeclared codes"),
        vec![ScVal::Error(ScError::Contract(99))]
    );
}

#[test]
fn existing_raw_scval_escape_hatch_for_generic_error_stays_available() {
    let value = ScVal::Error(ScError::Contract(99));
    assert_eq!(spec().encode(&value, &ScSpecTypeDef::Error).unwrap(), value);
    assert_eq!(
        spec()
            .encode_args("accept_generic_error", &(&value,))
            .unwrap(),
        vec![value]
    );
}

#[test]
fn error_vector_encodes_without_treating_elements_as_failures() {
    let values = vec![
        ScError::Contract(99),
        ScError::Auth(ScErrorCode::InvalidAction),
    ];
    let expected = vector(values.iter().cloned().map(ScVal::Error).collect());
    assert_eq!(
        spec()
            .encode(&values, &error_vec_type())
            .expect("error values in a vector should encode as data"),
        expected
    );
}

#[test]
fn error_vector_decodes_without_treating_elements_as_failures() {
    let values = vec![
        ScError::Contract(99),
        ScError::Auth(ScErrorCode::InvalidAction),
    ];
    let raw = vector(values.iter().cloned().map(ScVal::Error).collect());
    assert_eq!(
        spec()
            .decode::<Vec<ScError>>(&raw, &error_vec_type())
            .expect("error values in a vector should decode as data"),
        values
    );
}

#[test]
fn error_map_values_round_trip() {
    let values = BTreeMap::from([
        (1u32, ScError::Contract(99)),
        (2, ScError::Storage(ScErrorCode::MissingValue)),
    ]);
    let raw = ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            values
                .iter()
                .map(|(k, v)| (ScVal::U32(*k), ScVal::Error(v.clone()))),
        )
        .unwrap(),
    ));
    let spec = spec();
    assert_eq!(
        spec.encode(&values, &error_map_type())
            .expect("error-valued map entries should encode"),
        raw
    );
    assert_eq!(
        spec.decode::<BTreeMap<u32, ScError>>(&raw, &error_map_type())
            .expect("error-valued map entries should decode"),
        values
    );
}

#[test]
fn generic_errors_can_be_map_keys_without_losing_their_error_class() {
    let ty = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(ScSpecTypeDef::Error),
        value_type: Box::new(ScSpecTypeDef::U32),
    }));
    let values = BTreeMap::from([
        (ScError::Contract(7), 1u32),
        (ScError::Storage(ScErrorCode::MissingValue), 2),
    ]);
    let raw = ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            values
                .iter()
                .map(|(k, v)| (ScVal::Error(k.clone()), ScVal::U32(*v))),
        )
        .unwrap(),
    ));
    let spec = spec();
    assert_eq!(
        spec.encode(&values, &ty)
            .expect("generic errors should be valid typed map keys"),
        raw
    );
    assert_eq!(
        spec.decode::<BTreeMap<ScError, u32>>(&raw, &ty)
            .expect("error map keys should preserve class and code"),
        values
    );
}

#[test]
fn present_optional_error_round_trips() {
    let value = Some(ScError::Contract(99));
    let raw = ScVal::Error(ScError::Contract(99));
    let spec = spec();
    assert_eq!(
        spec.encode(&value, &error_option_type())
            .expect("Some(Error) should encode as data"),
        raw
    );
    assert_eq!(
        spec.decode::<Option<ScError>>(&raw, &error_option_type())
            .expect("Some(Error) should decode as data"),
        value
    );
}

#[test]
fn absent_optional_error_remains_none() {
    let spec = spec();
    assert_eq!(
        spec.encode(&None::<ScError>, &error_option_type()).unwrap(),
        ScVal::Void
    );
    assert_eq!(
        spec.decode::<Option<ScError>>(&ScVal::Void, &error_option_type())
            .unwrap(),
        None
    );
}

#[test]
fn error_record_encodes_nested_typed_and_generic_errors() {
    assert_eq!(
        spec()
            .encode(&record(), &udt("ErrorRecord"))
            .expect("error fields should encode as ordinary struct data"),
        error_record_value()
    );
}

#[test]
fn error_record_decodes_nested_typed_and_generic_errors() {
    assert_eq!(
        spec()
            .decode::<ErrorRecord>(&error_record_value(), &udt("ErrorRecord"))
            .expect("error fields should decode as ordinary struct data"),
        record()
    );
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

#[test]
fn generic_error_event_preserves_an_unknown_code() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct ErrorDatum {
        value: ScError,
    }
    let actual: ErrorDatum = spec()
        .decode_event_as(
            "ErrorDatum",
            &event("error_datum", ScVal::Error(ScError::Contract(99))),
        )
        .expect("a generic error in event data is not a failed invocation");
    assert_eq!(
        actual,
        ErrorDatum {
            value: ScError::Contract(99)
        }
    );
}

#[test]
fn generic_error_event_preserves_a_host_error_class() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct ErrorDatum {
        value: ScError,
    }
    let error = ScError::Storage(ScErrorCode::MissingValue);
    let actual: ErrorDatum = spec()
        .decode_event_as(
            "ErrorDatum",
            &event("error_datum", ScVal::Error(error.clone())),
        )
        .expect("a host-error-valued event parameter should preserve its error class");
    assert_eq!(actual, ErrorDatum { value: error });
}

#[test]
fn typed_error_event_decodes_its_declared_variant() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct TypedErrorDatum {
        value: ArithmeticFailure,
    }
    let actual: TypedErrorDatum = spec()
        .decode_event_as(
            "TypedErrorDatum",
            &event("typed_error_datum", ScVal::Error(ScError::Contract(7))),
        )
        .expect("a typed error event parameter should decode as its Rust variant");
    assert_eq!(
        actual,
        TypedErrorDatum {
            value: ArithmeticFailure::Overflow
        }
    );
}

#[test]
fn typed_error_event_rejects_an_unknown_code() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct TypedErrorDatum {
        value: ArithmeticFailure,
    }
    assert!(matches!(
        spec().decode_event_as::<TypedErrorDatum>(
            "TypedErrorDatum",
            &event("typed_error_datum", ScVal::Error(ScError::Contract(99)))
        ),
        Err(EventDecodeError::InvalidValue { .. })
    ));
}

#[test]
fn error_record_event_decodes_nested_error_data() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct ErrorReport {
        record: ErrorRecord,
    }
    let actual: ErrorReport = spec()
        .decode_event_as("ErrorReport", &event("error_report", error_record_value()))
        .expect("nested event errors should be ordinary data");
    assert_eq!(actual, ErrorReport { record: record() });
}

#[test]
fn all_host_error_classes_and_codes_round_trip() {
    let spec = spec();
    for code in ScErrorCode::VARIANTS {
        for error in [
            ScError::WasmVm(code),
            ScError::Context(code),
            ScError::Storage(code),
            ScError::Object(code),
            ScError::Crypto(code),
            ScError::Events(code),
            ScError::Budget(code),
            ScError::Value(code),
            ScError::Auth(code),
        ] {
            let raw = ScVal::Error(error.clone());
            assert_eq!(spec.encode(&error, &ScSpecTypeDef::Error).unwrap(), raw);
            assert_eq!(
                spec.decode::<ScError>(&raw, &ScSpecTypeDef::Error).unwrap(),
                error
            );
        }
    }
}

#[test]
fn errors_inside_tuples_and_union_payloads_use_native_conversion() {
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum ErrorChoice {
        One(ArithmeticFailure),
        Pair(u32, ScError),
    }
    let mut entries = entries();
    entries.push(ScSpecEntry::UdtUnionV0(ScSpecUdtUnionV0 {
        name: "ErrorChoice".try_into().unwrap(),
        cases: [
            ("One", vec![udt("ArithmeticFailure")]),
            ("Pair", vec![ScSpecTypeDef::U32, ScSpecTypeDef::Error]),
        ]
        .into_iter()
        .map(|(name, types)| {
            ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                name: name.try_into().unwrap(),
                type_: types.try_into().unwrap(),
                doc: Default::default(),
            })
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap(),
        ..Default::default()
    }));
    let spec = ContractSpec::new(entries).unwrap();
    for (value, raw) in [
        (
            ErrorChoice::One(ArithmeticFailure::Overflow),
            vector(vec![
                ScVal::Symbol("One".try_into().unwrap()),
                ScVal::Error(ScError::Contract(7)),
            ]),
        ),
        (
            ErrorChoice::Pair(42, ScError::Contract(99)),
            vector(vec![
                ScVal::Symbol("Pair".try_into().unwrap()),
                ScVal::U32(42),
                ScVal::Error(ScError::Contract(99)),
            ]),
        ),
    ] {
        assert_eq!(spec.encode(&value, &udt("ErrorChoice")).unwrap(), raw);
        assert_eq!(
            spec.decode::<ErrorChoice>(&raw, &udt("ErrorChoice"))
                .unwrap(),
            value
        );
    }
    let ty = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
        value_types: vec![ScSpecTypeDef::U32, ScSpecTypeDef::Error]
            .try_into()
            .unwrap(),
    }));
    let value = (42u32, ScError::Contract(99));
    let raw = vector(vec![ScVal::U32(42), ScVal::Error(ScError::Contract(99))]);
    assert_eq!(spec.encode(&value, &ty).unwrap(), raw);
    assert_eq!(spec.decode::<(u32, ScError)>(&raw, &ty).unwrap(), value);
}

#[test]
fn typed_error_map_keys_support_btree_and_hash_maps() {
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    enum Key {
        Overflow,
    }
    let ty = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(udt("ArithmeticFailure")),
        value_type: Box::new(ScSpecTypeDef::U32),
    }));
    let raw = ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            [(ScVal::Error(ScError::Contract(7)), ScVal::U32(42))].into_iter(),
        )
        .unwrap(),
    ));
    let spec = spec();
    let btree = BTreeMap::from([(Key::Overflow, 42u32)]);
    let hash = std::collections::HashMap::from([(Key::Overflow, 42u32)]);
    assert_eq!(spec.encode(&btree, &ty).unwrap(), raw);
    assert_eq!(spec.encode(&hash, &ty).unwrap(), raw);
    assert_eq!(spec.decode::<BTreeMap<Key, u32>>(&raw, &ty).unwrap(), btree);
    assert_eq!(
        spec.decode::<std::collections::HashMap<Key, u32>>(&raw, &ty)
            .unwrap(),
        hash
    );
}

#[test]
fn malformed_error_representations_are_rejected_without_panicking() {
    let spec = spec();
    for value in [
        serde_json::json!({"contract": -1}),
        serde_json::json!({"contract": 4294967296u64}),
        serde_json::json!({"storage": "not_a_code"}),
        serde_json::json!({"contract": 7, "storage": "missing_value"}),
        serde_json::json!({"error": {"u32": 7}}),
    ] {
        assert!(spec.encode(&value, &ScSpecTypeDef::Error).is_err());
    }
}
