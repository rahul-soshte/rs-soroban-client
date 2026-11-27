//! SDK unit values use Void even when their spec type is Tuple([]).
#![cfg(feature = "contract-spec")]
mod support;

use soroban_client::{contract_spec::ContractSpec, xdr::*};
use support::{udt, zero_tuple::*};

fn spec() -> ContractSpec {
    ContractSpec::new(support::entries()).unwrap()
}

#[test]
fn encode_unit_as_zero_tuple() {
    assert_eq!(
        spec()
            .encode(&(), &tuple_type())
            .expect("Tuple([]) should encode unit as Void"),
        ScVal::Void
    );
}

#[test]
fn decode_zero_tuple_from_void() {
    spec()
        .decode::<()>(&ScVal::Void, &tuple_type())
        .expect("Tuple([]) should decode Void as unit");
}

#[test]
fn unit_argument_is_one_void_argument_not_no_arguments() {
    assert_eq!(
        spec()
            .encode_args("echo_unit", &((),))
            .expect("a unit argument should encode using Tuple([])"),
        vec![ScVal::Void]
    );
}

#[test]
fn unit_argument_cannot_be_omitted() {
    assert!(spec().encode_args("echo_unit", &()).is_err());
}

#[test]
fn zero_tuple_function_result() {
    spec()
        .decode_result::<()>("echo_unit", &ScVal::Void)
        .expect("a Tuple([]) function result should decode as unit");
}

#[test]
fn zero_tuple_field_encodes_as_void() {
    assert_eq!(
        spec()
            .encode(&UnitRecord { value: () }, &udt("UnitRecord"))
            .expect("a Tuple([]) field should encode as Void"),
        record_value()
    );
}

#[test]
fn zero_tuple_field_decodes_from_void() {
    assert_eq!(
        spec()
            .decode::<UnitRecord>(&record_value(), &udt("UnitRecord"))
            .expect("a Tuple([]) field should decode from Void"),
        UnitRecord { value: () }
    );
}

#[test]
fn zero_tuple_event_parameter() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct UnitDatum {
        value: (),
    }
    let event = ContractEvent {
        contract_id: Some(ContractId(Hash([0; 32]))),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: vec![ScVal::Symbol("unit_datum".try_into().unwrap())]
                .try_into()
                .unwrap(),
            data: ScVal::Void,
        }),
        ..Default::default()
    };
    assert_eq!(
        spec()
            .decode_event_as::<UnitDatum>("UnitDatum", &event)
            .expect("a Tuple([]) event parameter should decode from Void"),
        UnitDatum { value: () }
    );
}

#[test]
fn explicit_void_type_still_round_trips_unit() {
    let spec = spec();
    assert_eq!(spec.encode(&(), &ScSpecTypeDef::Void).unwrap(), ScVal::Void);
    spec.decode::<()>(&ScVal::Void, &ScSpecTypeDef::Void)
        .unwrap();
}

#[test]
fn void_is_not_a_nonempty_tuple() {
    let ty = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
        value_types: vec![ScSpecTypeDef::U32].try_into().unwrap(),
    }));
    assert!(spec().decode::<(u32,)>(&ScVal::Void, &ty).is_err());
}

#[test]
fn zero_tuple_rejects_nonunit_payloads() {
    let spec = spec();
    for raw in [
        ScVal::U32(0),
        ScVal::Bool(false),
        ScVal::Vec(Some(Default::default())),
        ScVal::Vec(Some(vec![ScVal::U32(0)].try_into().unwrap())),
    ] {
        assert!(spec.decode::<()>(&raw, &tuple_type()).is_err());
    }
}

#[test]
fn zero_tuple_rejects_nonunit_rust_values() {
    let spec = spec();
    assert!(spec.encode(&Vec::<u32>::new(), &tuple_type()).is_err());
    assert!(spec.encode(&0u32, &tuple_type()).is_err());
}

#[test]
fn zero_tuples_nested_in_containers_round_trip() {
    use std::{collections::BTreeMap, fmt::Debug};
    fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + Debug>(
        value: T,
        raw: ScVal,
        ty: ScSpecTypeDef,
    ) {
        let spec = spec();
        assert_eq!(spec.encode(&value, &ty).unwrap(), raw);
        assert_eq!(spec.decode::<T>(&raw, &ty).unwrap(), value);
    }
    round_trip(
        vec![(), ()],
        ScVal::Vec(Some(vec![ScVal::Void, ScVal::Void].try_into().unwrap())),
        ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(tuple_type()),
        })),
    );
    round_trip(
        ((), 42u32),
        ScVal::Vec(Some(vec![ScVal::Void, ScVal::U32(42)].try_into().unwrap())),
        ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
            value_types: vec![tuple_type(), ScSpecTypeDef::U32].try_into().unwrap(),
        })),
    );
    round_trip(
        BTreeMap::from([((), 42u32)]),
        ScVal::Map(Some(
            ScMap::sorted_from_pairs([(ScVal::Void, ScVal::U32(42))].into_iter()).unwrap(),
        )),
        ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
            key_type: Box::new(tuple_type()),
            value_type: Box::new(ScSpecTypeDef::U32),
        })),
    );
    round_trip(
        Ok::<(), ScError>(()),
        ScVal::Void,
        ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
            ok_type: Box::new(tuple_type()),
            error_type: Box::new(ScSpecTypeDef::Error),
        })),
    );
}
