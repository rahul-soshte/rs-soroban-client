#![cfg(feature = "contract-spec")]
mod support;

use serde::{Deserialize, Serialize};
use soroban_client::{
    address::{Address, AddressTrait},
    contract_spec::ContractSpec,
    xdr::*,
};
use std::collections::BTreeMap;
use support::*;

fn vector(values: Vec<ScVal>) -> ScVal {
    ScVal::Vec(Some(values.try_into().unwrap()))
}
fn raw_map() -> ScVal {
    ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            [
                (
                    vector(vec![ScVal::U32(1), ScVal::U32(2)]),
                    generic_val_value(),
                ),
                (
                    ScVal::Symbol("value".try_into().unwrap()),
                    ScVal::from(i128::MIN),
                ),
            ]
            .into_iter(),
        )
        .unwrap(),
    ))
}
fn vec_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
        element_type: Box::new(ScSpecTypeDef::Val),
    }))
}
fn map_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(ScSpecTypeDef::Val),
        value_type: Box::new(ScSpecTypeDef::Val),
    }))
}
fn option_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
        value_type: Box::new(ScSpecTypeDef::Val),
    }))
}
fn spec() -> ContractSpec {
    let mut entries = entries();
    entries.push(ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
        name: "ValRecord".try_into().unwrap(),
        fields: [
            ("value", ScSpecTypeDef::Val),
            ("values", vec_type()),
            ("keyed", map_type()),
            ("optional", option_type()),
        ]
        .into_iter()
        .map(|(name, type_)| ScSpecUdtStructFieldV0 {
            name: name.try_into().unwrap(),
            type_,
            doc: Default::default(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap(),
        ..Default::default()
    }));
    entries.push(ScSpecEntry::UdtUnionV0(ScSpecUdtUnionV0 {
        name: "ValChoice".try_into().unwrap(),
        cases: [("One", ScSpecTypeDef::Val), ("Many", vec_type())]
            .into_iter()
            .map(|(name, ty)| {
                ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                    name: name.try_into().unwrap(),
                    type_: vec![ty].try_into().unwrap(),
                    doc: Default::default(),
                })
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        ..Default::default()
    }));
    entries.push(ScSpecEntry::EventV0(ScSpecEventV0 {
        name: "GenericChanged".try_into().unwrap(),
        prefix_topics: vec!["generic_changed".try_into().unwrap()]
            .try_into()
            .unwrap(),
        params: vec![ScSpecEventParamV0 {
            name: "value".try_into().unwrap(),
            type_: ScSpecTypeDef::Val,
            location: ScSpecEventParamLocationV0::Data,
            doc: Default::default(),
        }]
        .try_into()
        .unwrap(),
        data_format: ScSpecEventDataFormat::SingleValue,
        ..Default::default()
    }));
    ContractSpec::new(entries).unwrap()
}

// Separate tests make each lost tag visible rather than stopping at the first
// unsupported value in a single round-trip loop. Error-valued data is separate.
macro_rules! val_decode {
    ($name:ident,$value:expr) => {
        #[test]
        fn $name() {
            let expected: ScVal = $value;
            let decoded: ScVal = spec()
                .decode(&expected, &ScSpecTypeDef::Val)
                .expect("generic Val decoding should preserve the explicit ScVal");
            assert_eq!(decoded, expected);
        }
    };
}
val_decode!(decode_void, ScVal::Void);
val_decode!(
    decode_contract_error_data,
    ScVal::Error(ScError::Contract(99))
);
val_decode!(
    decode_host_error_data,
    ScVal::Error(ScError::Storage(ScErrorCode::MissingValue))
);
val_decode!(decode_bool, ScVal::Bool(true));
val_decode!(decode_u32, ScVal::U32(7));
val_decode!(decode_i32, ScVal::I32(-7));
val_decode!(decode_u64, ScVal::U64(u64::MAX));
val_decode!(decode_i64, ScVal::I64(i64::MIN));
val_decode!(decode_u128, ScVal::from(u128::MAX));
val_decode!(decode_i128, ScVal::from(i128::MIN));
val_decode!(
    decode_u256,
    ScVal::U256(UInt256Parts {
        hi_hi: u64::MAX,
        hi_lo: u64::MAX,
        lo_hi: u64::MAX,
        lo_lo: u64::MAX
    })
);
val_decode!(
    decode_i256,
    ScVal::I256(Int256Parts {
        hi_hi: i64::MIN,
        hi_lo: 0,
        lo_hi: 0,
        lo_lo: 0
    })
);
val_decode!(decode_timepoint, ScVal::Timepoint(TimePoint(u64::MAX)));
val_decode!(decode_duration, ScVal::Duration(Duration(u64::MAX)));
val_decode!(
    decode_bytes,
    ScVal::Bytes(vec![0u8, 128, 255].try_into().unwrap())
);
val_decode!(
    decode_string_without_guessing_symbol,
    ScVal::String(ScString("same".try_into().unwrap()))
);
val_decode!(
    decode_symbol_without_guessing_string,
    ScVal::Symbol("same".try_into().unwrap())
);
val_decode!(
    decode_account_address,
    Address::new(ACCOUNT).unwrap().to_sc_val().unwrap()
);
val_decode!(
    decode_contract_address,
    Address::new(CONTRACT).unwrap().to_sc_val().unwrap()
);
val_decode!(decode_heterogeneous_vector, generic_val_value());
val_decode!(decode_map_with_composite_keys, raw_map());

#[test]
fn explicit_scval_arguments_keep_their_existing_wire_encoding() {
    let spec = spec();
    for value in [
        ScVal::Void,
        ScVal::Bool(true),
        ScVal::U32(7),
        ScVal::from(u128::MAX),
        ScVal::from(i128::MIN),
        ScVal::String(ScString("same".try_into().unwrap())),
        ScVal::Symbol("same".try_into().unwrap()),
        generic_val_value(),
        raw_map(),
    ] {
        assert_eq!(spec.encode(&value, &ScSpecTypeDef::Val).unwrap(), value);
        assert_eq!(
            spec.encode_args("echo_val", &(&value,)).unwrap(),
            vec![value]
        );
    }
}

#[test]
fn function_result_can_be_requested_as_scval() {
    let expected = generic_val_value();
    let actual: ScVal = spec()
        .decode_result("echo_val", &expected)
        .expect("a generic Val function result should decode as ScVal");
    assert_eq!(actual, expected);
}

#[test]
fn vec_of_val_round_trips_as_vec_of_scval() {
    let values = vec![ScVal::U32(7), ScVal::I32(7), generic_val_value(), raw_map()];
    let expected = vector(values.clone());
    let spec = spec();
    assert_eq!(spec.encode(&values, &vec_type()).unwrap(), expected);
    assert_eq!(
        spec.decode::<Vec<ScVal>>(&expected, &vec_type())
            .expect("Vec<Val> should decode as Vec<ScVal>"),
        values
    );
}

#[test]
fn map_of_val_round_trips_as_map_of_scval() {
    let values = BTreeMap::from([
        (
            ScVal::String(ScString("same".try_into().unwrap())),
            ScVal::U32(7),
        ),
        (
            ScVal::Symbol("same".try_into().unwrap()),
            generic_val_value(),
        ),
        (vector(vec![ScVal::U32(1), ScVal::U32(2)]), raw_map()),
    ]);
    let expected = ScVal::Map(Some(
        ScMap::sorted_from_pairs(values.clone().into_iter()).unwrap(),
    ));
    let spec = spec();
    assert_eq!(spec.encode(&values, &map_type()).unwrap(), expected);
    assert_eq!(
        spec.decode::<BTreeMap<ScVal, ScVal>>(&expected, &map_type())
            .expect("Map<Val, Val> should preserve both key and value tags"),
        values
    );
}

#[test]
fn present_option_of_val_round_trips() {
    let value = Some(ScVal::U64(u64::MAX));
    let expected = value.as_ref().unwrap().clone();
    let spec = spec();
    assert_eq!(spec.encode(&value, &option_type()).unwrap(), expected);
    assert_eq!(
        spec.decode::<Option<ScVal>>(&expected, &option_type())
            .expect("Some(non-void Val) should preserve its ScVal"),
        value
    );
}

#[test]
fn absent_option_of_val_remains_none() {
    let spec = spec();
    assert_eq!(
        spec.encode(&None::<ScVal>, &option_type()).unwrap(),
        ScVal::Void
    );
    assert_eq!(
        spec.decode::<Option<ScVal>>(&ScVal::Void, &option_type())
            .unwrap(),
        None
    );
    // Some(ScVal::Void) is intentionally excluded: the wire cannot distinguish it from None.
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ValRecord {
    value: ScVal,
    values: Vec<ScVal>,
    keyed: BTreeMap<ScVal, ScVal>,
    optional: Option<ScVal>,
}

#[test]
fn generic_values_nested_in_a_struct_round_trip() {
    let value = ValRecord {
        value: generic_val_value(),
        values: vec![ScVal::Bool(true), raw_map()],
        keyed: BTreeMap::from([(ScVal::U32(1), ScVal::Symbol("same".try_into().unwrap()))]),
        optional: Some(ScVal::I32(-7)),
    };
    let expected = ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            [
                (
                    ScVal::Symbol("value".try_into().unwrap()),
                    value.value.clone(),
                ),
                (
                    ScVal::Symbol("values".try_into().unwrap()),
                    vector(value.values.clone()),
                ),
                (
                    ScVal::Symbol("keyed".try_into().unwrap()),
                    ScVal::Map(Some(
                        ScMap::sorted_from_pairs(value.keyed.clone().into_iter()).unwrap(),
                    )),
                ),
                (
                    ScVal::Symbol("optional".try_into().unwrap()),
                    value.optional.as_ref().unwrap().clone(),
                ),
            ]
            .into_iter(),
        )
        .unwrap(),
    ));
    let spec = spec();
    assert_eq!(spec.encode(&value, &udt("ValRecord")).unwrap(), expected);
    assert_eq!(
        spec.decode::<ValRecord>(&expected, &udt("ValRecord"))
            .expect("generic fields should preserve their explicit ScVal representation"),
        value
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum ValChoice {
    One(ScVal),
    Many(Vec<ScVal>),
}

#[test]
fn generic_values_nested_in_union_variants_round_trip() {
    let spec = spec();
    for (value, expected) in [
        (
            ValChoice::One(raw_map()),
            vector(vec![ScVal::Symbol("One".try_into().unwrap()), raw_map()]),
        ),
        (
            ValChoice::Many(vec![generic_val_value()]),
            vector(vec![
                ScVal::Symbol("Many".try_into().unwrap()),
                vector(vec![generic_val_value()]),
            ]),
        ),
    ] {
        assert_eq!(spec.encode(&value, &udt("ValChoice")).unwrap(), expected);
        assert_eq!(
            spec.decode::<ValChoice>(&expected, &udt("ValChoice"))
                .expect("generic union payloads should round-trip"),
            value
        );
    }
}

#[test]
fn generic_event_parameter_decodes_as_scval() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct GenericChanged {
        value: ScVal,
    }
    let value = generic_val_value();
    let event = ContractEvent {
        contract_id: Some(ContractId(Hash([0; 32]))),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: vec![ScVal::Symbol("generic_changed".try_into().unwrap())]
                .try_into()
                .unwrap(),
            data: value.clone(),
        }),
        ..Default::default()
    };
    let actual: GenericChanged = spec()
        .decode_event_as("GenericChanged", &event)
        .expect("a generic Val event parameter should decode as ScVal");
    assert_eq!(actual, GenericChanged { value });
}

#[test]
fn generic_val_can_contain_nested_error_data() {
    let spec = spec();
    let value = vector(vec![ScVal::Error(ScError::Contract(7)), raw_map()]);
    assert_eq!(spec.encode(&value, &ScSpecTypeDef::Val).unwrap(), value);
    assert_eq!(
        spec.decode::<ScVal>(&value, &ScSpecTypeDef::Val).unwrap(),
        value
    );
}

#[test]
fn hash_maps_preserve_generic_val_keys_and_values() {
    let values = std::collections::HashMap::from([
        (
            ScVal::String(ScString("same".try_into().unwrap())),
            ScVal::U32(7),
        ),
        (ScVal::Symbol("same".try_into().unwrap()), ScVal::I32(7)),
    ]);
    let expected = ScVal::Map(Some(
        ScMap::sorted_from_pairs(values.clone().into_iter()).unwrap(),
    ));
    let spec = spec();
    assert_eq!(spec.encode(&values, &map_type()).unwrap(), expected);
    assert_eq!(
        spec.decode::<std::collections::HashMap<ScVal, ScVal>>(&expected, &map_type())
            .unwrap(),
        values
    );
}

#[test]
fn malformed_and_internal_generic_values_are_rejected() {
    let spec = spec();
    for value in [
        ScVal::LedgerKeyContractInstance,
        vector(vec![ScVal::LedgerKeyContractInstance]),
        ScVal::Vec(None),
        ScVal::Map(None),
    ] {
        assert!(spec.encode(&value, &ScSpecTypeDef::Val).is_err());
        assert!(spec.decode::<ScVal>(&value, &ScSpecTypeDef::Val).is_err());
    }
}

#[test]
fn generic_val_maps_must_be_canonical_even_when_nested() {
    let spec = spec();
    for keys in [[2u32, 1u32], [1, 1]] {
        let raw = ScVal::Map(Some(
            keys.into_iter()
                .map(|key| ScMapEntry {
                    key: ScVal::U32(key),
                    val: ScVal::Void,
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        ));
        for value in [raw.clone(), vector(vec![raw])] {
            assert!(spec.encode(&value, &ScSpecTypeDef::Val).is_err());
            assert!(spec.decode::<ScVal>(&value, &ScSpecTypeDef::Val).is_err());
        }
    }
}
