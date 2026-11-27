#![cfg(feature = "contract-spec")]
mod support;

use serde::{Deserialize, Serialize};
use soroban_client::{
    contract_spec::{ContractSpec, EventDecodeError, U256},
    soroban_rpc::EventResponse,
    xdr::*,
};
use std::collections::BTreeMap;
use support::*;

fn symbol(s: &str) -> ScVal {
    ScVal::Symbol(s.try_into().unwrap())
}
fn string(s: &str) -> ScVal {
    ScVal::String(ScString(s.try_into().unwrap()))
}
fn vector(values: Vec<ScVal>) -> ScVal {
    ScVal::Vec(Some(values.try_into().unwrap()))
}
fn map(values: Vec<(&str, ScVal)>) -> ScVal {
    ScVal::Map(Some(
        ScMap::sorted_from_pairs(values.into_iter().map(|(k, v)| (symbol(k), v))).unwrap(),
    ))
}
fn definition(
    name: &str,
    prefix: &[&str],
    params: &[(&str, ScSpecTypeDef, ScSpecEventParamLocationV0)],
    format: ScSpecEventDataFormat,
) -> ScSpecEventV0 {
    ScSpecEventV0 {
        name: name.try_into().unwrap(),
        prefix_topics: prefix
            .iter()
            .map(|p| (*p).try_into().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        params: params
            .iter()
            .map(|(name, ty, location)| ScSpecEventParamV0 {
                name: (*name).try_into().unwrap(),
                type_: ty.clone(),
                location: *location,
                doc: Default::default(),
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        data_format: format,
        ..Default::default()
    }
}
fn spec(events: Vec<ScSpecEventV0>) -> ContractSpec {
    let mut entries: Vec<_> = entries()
        .into_iter()
        .filter(|e| !matches!(e, ScSpecEntry::EventV0(_)))
        .collect();
    entries.extend(events.into_iter().map(ScSpecEntry::EventV0));
    ContractSpec::new(entries).unwrap()
}
fn event(topics: Vec<ScVal>, data: ScVal) -> ContractEvent {
    ContractEvent {
        type_: ContractEventType::Contract,
        contract_id: Some(ContractId(Hash([0; 32]))),
        body: ContractEventBody::V0(ContractEventV0 {
            topics: topics.try_into().unwrap(),
            data,
        }),
        ..Default::default()
    }
}
fn rpc_event(event: &ContractEvent) -> EventResponse {
    let ContractEventBody::V0(body) = &event.body;
    serde_json::from_value(serde_json::json!({
        "type":"contract", "contractId":CONTRACT, "ledger":100,"ledgerClosedAt":"2026-11-24T00:00:00Z",
        "id":"1-1","txHash":"test", "topic":body.topics.iter().map(|v| v.to_xdr_base64(Limits::none()).unwrap()).collect::<Vec<_>>(),
        "value":body.data.to_xdr_base64(Limits::none()).unwrap(),
    })).unwrap()
}
fn single(name: &str, ty: ScSpecTypeDef) -> ScSpecEventV0 {
    definition(
        name,
        &["changed"],
        &[("value", ty, ScSpecEventParamLocationV0::Data)],
        ScSpecEventDataFormat::SingleValue,
    )
}

#[derive(Debug, PartialEq, Deserialize)]
struct Changed {
    value: u32,
}

#[test]
fn decodes_single_value_from_rpc_xdr_and_simulation() {
    let spec = spec(vec![single("Changed", ScSpecTypeDef::U32)]);
    let raw = event(vec![symbol("changed")], ScVal::U32(7));
    assert_eq!(
        spec.decode_event::<Changed>(&raw).unwrap(),
        Changed { value: 7 }
    );
    let rpc = rpc_event(&raw);
    assert_eq!(
        spec.decode_event::<Changed>(&rpc).unwrap(),
        Changed { value: 7 }
    );
    let wrapped = DiagnosticEvent {
        in_successful_contract_call: true,
        event: raw,
    };
    assert_eq!(
        spec.decode_event::<Changed>(&wrapped).unwrap(),
        Changed { value: 7 }
    );
    assert_eq!(spec.event("Changed").unwrap().name.as_slice(), b"Changed");
}

#[test]
fn prefix_symbols_and_strings_are_interchangeable_only_for_static_topics() {
    use ScSpecEventParamLocationV0 as L;
    let spec = spec(vec![definition(
        "Tagged",
        &["changed", "v1"],
        &[
            ("tag", ScSpecTypeDef::Symbol, L::TopicList),
            ("value", ScSpecTypeDef::U32, L::Data),
        ],
        ScSpecEventDataFormat::SingleValue,
    )]);
    #[derive(Debug, PartialEq, Deserialize)]
    struct Tagged {
        tag: String,
        value: u32,
    }
    let raw = event(
        vec![string("changed"), symbol("v1"), symbol("tag")],
        ScVal::U32(7),
    );
    assert_eq!(
        spec.decode_event::<Tagged>(&raw).unwrap(),
        Tagged {
            tag: "tag".into(),
            value: 7
        }
    );
    let raw = event(
        vec![symbol("changed"), string("v1"), string("tag")],
        ScVal::U32(7),
    );
    assert!(matches!(
        spec.decode_event::<Tagged>(&raw),
        Err(EventDecodeError::InvalidValue { .. })
    ));
    for topics in [vec![symbol("wrong")], vec![ScVal::U32(1)], vec![]] {
        assert!(matches!(
            spec.decode_event::<Tagged>(&event(topics, ScVal::U32(7))),
            Err(EventDecodeError::NoMatchingEvent)
        ));
    }
}

#[test]
fn decodes_interleaved_topic_and_data_params_in_all_formats() {
    use ScSpecEventParamLocationV0 as L;
    use ScSpecTypeDef as T;
    #[derive(Debug, PartialEq, Deserialize)]
    struct Transfer {
        amount: i128,
        from: String,
        memo: String,
    }
    let params = [
        ("amount", T::I128, L::Data),
        ("from", T::Address, L::TopicList),
        ("memo", T::String, L::Data),
    ];
    let from = soroban_client::address::Address::new(ACCOUNT).unwrap();
    use soroban_client::address::AddressTrait;
    for (format, data) in [
        (
            ScSpecEventDataFormat::Vec,
            vector(vec![ScVal::from(i128::MIN), string("hello")]),
        ),
        (
            ScSpecEventDataFormat::Map,
            map(vec![
                ("memo", string("hello")),
                ("amount", ScVal::from(i128::MIN)),
            ]),
        ),
    ] {
        let spec = spec(vec![definition("Transfer", &["transfer"], &params, format)]);
        let raw = event(vec![symbol("transfer"), from.to_sc_val().unwrap()], data);
        let value: Transfer = spec.decode_event(&raw).unwrap();
        assert_eq!(
            value,
            Transfer {
                amount: i128::MIN,
                from: ACCOUNT.into(),
                memo: "hello".into()
            }
        );
    }
}

#[test]
fn matches_shared_prefix_using_the_complete_wire_shape() {
    let spec = spec(vec![
        single("Text", ScSpecTypeDef::String),
        single("Integer", ScSpecTypeDef::U32),
    ]);
    let raw = event(vec![symbol("changed")], ScVal::U32(7));
    assert_eq!(
        spec.decode_event::<Changed>(&raw).unwrap(),
        Changed { value: 7 }
    );
    assert!(matches!(
        spec.decode_event_as::<Changed>("Text", &raw),
        Err(EventDecodeError::InvalidValue { .. })
    ));
    assert!(matches!(
        spec.decode_event::<Changed>(&event(vec![symbol("changed")], ScVal::Bool(true))),
        Err(EventDecodeError::NoMatchingShape { .. })
    ));
    assert!(matches!(
        spec.decode_event_as::<Changed>("Missing", &raw),
        Err(EventDecodeError::UnknownEvent(_))
    ));
    assert!(matches!(
        spec.decode_event_as::<Changed>("Integer", &event(vec![symbol("other")], ScVal::U32(7))),
        Err(EventDecodeError::InvalidLayout { .. })
    ));
}

#[test]
fn shared_prefix_matching_checks_nested_udt_parameter_types() {
    let make_struct = |name: &str, ty: ScSpecTypeDef| {
        ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            name: name.try_into().unwrap(),
            fields: vec![ScSpecUdtStructFieldV0 {
                name: "item".try_into().unwrap(),
                type_: ty,
                doc: Default::default(),
            }]
            .try_into()
            .unwrap(),
            ..Default::default()
        })
    };
    let mut entries = entries();
    entries.retain(|e| !matches!(e, ScSpecEntry::EventV0(_)));
    entries.extend([
        make_struct("TextPayload", ScSpecTypeDef::String),
        make_struct("IntPayload", ScSpecTypeDef::U32),
        ScSpecEntry::EventV0(single("AsText", udt("TextPayload"))),
        ScSpecEntry::EventV0(single("AsInt", udt("IntPayload"))),
    ]);
    let spec = ContractSpec::new(entries).unwrap();
    #[derive(Debug, PartialEq, Deserialize)]
    struct Payload {
        item: u32,
    }
    #[derive(Debug, PartialEq, Deserialize)]
    struct Outer {
        value: Payload,
    }
    let raw = event(vec![symbol("changed")], map(vec![("item", ScVal::U32(7))]));
    assert_eq!(
        spec.decode_event::<Outer>(&raw).unwrap(),
        Outer {
            value: Payload { item: 7 }
        }
    );
    assert!(matches!(
        spec.decode_event_as::<Outer>("AsText", &raw),
        Err(EventDecodeError::InvalidValue { .. })
    ));
}

#[test]
fn ambiguous_events_require_an_explicit_name_even_if_rust_type_matches_one() {
    let first = single("First", ScSpecTypeDef::U32);
    let mut second = single("Second", ScSpecTypeDef::U32);
    let mut params = second.params.to_vec();
    params[0].name = "other".try_into().unwrap();
    second.params = params.try_into().unwrap();
    let spec = spec(vec![first, second]);
    let raw = event(vec![symbol("changed")], ScVal::U32(7));
    match spec.decode_event::<Changed>(&raw) {
        Err(EventDecodeError::Ambiguous { events }) => assert_eq!(events, vec!["First", "Second"]),
        result => panic!("unexpected {result:?}"),
    }
    assert_eq!(
        spec.decode_event_as::<Changed>("First", &raw).unwrap(),
        Changed { value: 7 }
    );
    assert!(matches!(
        spec.decode_event_as::<Changed>("Second", &raw),
        Err(EventDecodeError::Deserialize(_))
    ));
}

#[test]
fn native_conversion_capabilities_do_not_resolve_wildcard_ambiguity() {
    let spec = spec(vec![
        single("Generic", ScSpecTypeDef::Val),
        single("Integer", ScSpecTypeDef::U32),
    ]);
    let raw = event(vec![symbol("changed")], ScVal::U32(7));
    assert!(matches!(
        spec.decode_event::<Changed>(&raw),
        Err(EventDecodeError::Ambiguous { .. })
    ));
    assert_eq!(
        spec.decode_event_as::<Changed>("Integer", &raw).unwrap(),
        Changed { value: 7 }
    );
}

#[test]
fn error_values_participate_in_wire_matching_before_native_deserialization() {
    let spec = spec(vec![
        single("Generic", ScSpecTypeDef::Val),
        single("Error", ScSpecTypeDef::Error),
    ]);
    let raw = event(vec![symbol("changed")], ScVal::Error(ScError::Contract(7)));
    assert!(matches!(
        spec.decode_event::<Changed>(&raw),
        Err(EventDecodeError::Ambiguous { .. })
    ));
    assert!(matches!(
        spec.decode_event_as::<Changed>("Error", &raw),
        Err(EventDecodeError::Deserialize(_))
    ));
    #[derive(Debug, PartialEq, Deserialize)]
    struct ErrorDatum {
        value: ScError,
    }
    assert_eq!(
        spec.decode_event_as::<ErrorDatum>("Error", &raw).unwrap(),
        ErrorDatum {
            value: ScError::Contract(7)
        }
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Record {
    z: i128,
    a: String,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Choice {
    Empty,
    One(u128),
    Pair(u32, String),
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Color {
    Red,
    Green,
    Blue,
}
#[derive(Debug, PartialEq, Deserialize)]
struct Complex {
    record: Record,
    choices: Vec<Choice>,
    colors: BTreeMap<(u32, u32), Color>,
    large: U256,
    maybe: Option<i128>,
}

#[test]
fn event_values_reuse_native_udt_map_and_large_integer_conversion() {
    use ScSpecEventParamLocationV0 as L;
    use ScSpecTypeDef as T;
    let choices = T::Vec(Box::new(ScSpecTypeVec {
        element_type: Box::new(udt("Choice")),
    }));
    let colors = T::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(T::Tuple(Box::new(ScSpecTypeTuple {
            value_types: vec![T::U32, T::U32].try_into().unwrap(),
        }))),
        value_type: Box::new(udt("Color")),
    }));
    let maybe = T::Option(Box::new(ScSpecTypeOption {
        value_type: Box::new(T::I128),
    }));
    let spec = spec(vec![definition(
        "Complex",
        &["complex"],
        &[
            ("record", udt("Record"), L::TopicList),
            ("choices", choices.clone(), L::Data),
            ("colors", colors.clone(), L::Data),
            ("large", T::U256, L::Data),
            ("maybe", maybe.clone(), L::Data),
        ],
        ScSpecEventDataFormat::Map,
    )]);
    let expected = Complex {
        record: Record {
            z: i128::MIN,
            a: "hello".into(),
        },
        choices: vec![
            Choice::Empty,
            Choice::One(u128::MAX),
            Choice::Pair(7, "hi".into()),
        ],
        colors: BTreeMap::from([((1, 2), Color::Green)]),
        large: U256::MAX,
        maybe: None,
    };
    let raw = event(
        vec![
            symbol("complex"),
            spec.encode(&expected.record, &udt("Record")).unwrap(),
        ],
        map(vec![
            ("choices", spec.encode(&expected.choices, &choices).unwrap()),
            ("colors", spec.encode(&expected.colors, &colors).unwrap()),
            ("large", spec.encode(&expected.large, &T::U256).unwrap()),
            ("maybe", spec.encode(&expected.maybe, &maybe).unwrap()),
        ]),
    );
    assert_eq!(spec.decode_event::<Complex>(&raw).unwrap(), expected);
}

#[test]
fn rejects_extra_missing_and_malformed_parameters() {
    use ScSpecEventParamLocationV0 as L;
    let single_spec = spec(vec![single("Changed", ScSpecTypeDef::U32)]);
    for raw in [
        event(vec![symbol("changed"), symbol("extra")], ScVal::U32(7)),
        event(vec![symbol("changed")], ScVal::I32(7)),
    ] {
        assert!(single_spec.decode_event::<Changed>(&raw).is_err());
    }
    for format in [ScSpecEventDataFormat::Vec, ScSpecEventDataFormat::Map] {
        let spec = spec(vec![definition(
            "Changed",
            &["changed"],
            &[("value", ScSpecTypeDef::U32, L::Data)],
            format,
        )]);
        for data in [
            ScVal::Void,
            vector(vec![]),
            vector(vec![ScVal::U32(7), ScVal::U32(8)]),
            map(vec![]),
            map(vec![("other", ScVal::U32(7))]),
            map(vec![("value", ScVal::U32(7)), ("extra", ScVal::U32(8))]),
        ] {
            assert!(spec
                .decode_event::<Changed>(&event(vec![symbol("changed")], data))
                .is_err());
        }
    }
    let spec = spec(vec![definition(
        "Changed",
        &["changed"],
        &[("value", ScSpecTypeDef::U32, L::TopicList)],
        ScSpecEventDataFormat::SingleValue,
    )]);
    assert!(spec
        .decode_event::<Changed>(&event(vec![symbol("changed")], ScVal::Void))
        .is_err());
}

#[test]
fn rejects_invalid_named_data_maps() {
    use ScSpecEventParamLocationV0 as L;
    let spec = spec(vec![definition(
        "Pair",
        &["pair"],
        &[
            ("a", ScSpecTypeDef::U32, L::Data),
            ("b", ScSpecTypeDef::U32, L::Data),
        ],
        ScSpecEventDataFormat::Map,
    )]);
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Pair {
        a: u32,
        b: u32,
    }
    for keys in [
        vec![symbol("b"), symbol("a")],
        vec![symbol("a"), symbol("a")],
        vec![string("a"), string("b")],
    ] {
        let data = ScVal::Map(Some(
            keys.into_iter()
                .map(|key| ScMapEntry {
                    key,
                    val: ScVal::U32(1),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        ));
        assert!(matches!(
            spec.decode_event::<Pair>(&event(vec![symbol("pair")], data)),
            Err(EventDecodeError::InvalidLayout { .. })
        ));
    }
}

#[test]
fn zero_data_parameters_require_the_declared_empty_representation() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct Empty {}
    for (format, data) in [
        (ScSpecEventDataFormat::SingleValue, ScVal::Void),
        (ScSpecEventDataFormat::Vec, vector(vec![])),
        (ScSpecEventDataFormat::Map, map(vec![])),
    ] {
        let spec = spec(vec![definition("Empty", &[], &[], format)]);
        assert_eq!(
            spec.decode_event::<Empty>(&event(vec![], data)).unwrap(),
            Empty {}
        );
        assert!(spec
            .decode_event::<Empty>(&event(vec![], ScVal::U32(1)))
            .is_err());
    }
}

#[test]
fn rejects_invalid_event_definitions() {
    use ScSpecEventParamLocationV0 as L;
    let duplicate = definition(
        "Duplicate",
        &["x"],
        &[
            ("value", ScSpecTypeDef::U32, L::TopicList),
            ("value", ScSpecTypeDef::U32, L::Data),
        ],
        ScSpecEventDataFormat::SingleValue,
    );
    let multiple = definition(
        "Multiple",
        &["x"],
        &[
            ("a", ScSpecTypeDef::U32, L::Data),
            ("b", ScSpecTypeDef::U32, L::Data),
        ],
        ScSpecEventDataFormat::SingleValue,
    );
    let mut invalid_utf8 = single("InvalidUtf8", ScSpecTypeDef::U32);
    let mut params = invalid_utf8.params.to_vec();
    params[0].name = vec![0xff].try_into().unwrap();
    invalid_utf8.params = params.try_into().unwrap();
    for definition in [duplicate, multiple, invalid_utf8] {
        let topics = definition
            .prefix_topics
            .iter()
            .cloned()
            .map(ScVal::Symbol)
            .collect();
        assert!(matches!(
            spec(vec![definition]).decode_event::<Changed>(&event(topics, ScVal::U32(7))),
            Err(EventDecodeError::InvalidSpec { .. })
        ));
    }
}

#[test]
fn rejects_noncontract_failed_or_malformed_event_sources() {
    let spec = spec(vec![single("Changed", ScSpecTypeDef::U32)]);
    let raw = event(vec![symbol("changed")], ScVal::U32(7));
    for kind in [ContractEventType::System, ContractEventType::Diagnostic] {
        let mut wrong = raw.clone();
        wrong.type_ = kind;
        assert!(matches!(
            spec.decode_event::<Changed>(&wrong),
            Err(EventDecodeError::NotContractEvent)
        ));
    }
    let mut missing = raw.clone();
    missing.contract_id = None;
    assert!(matches!(
        spec.decode_event::<Changed>(&missing),
        Err(EventDecodeError::InvalidContractId)
    ));
    let failed = DiagnosticEvent {
        in_successful_contract_call: false,
        event: raw.clone(),
    };
    assert!(matches!(
        spec.decode_event::<Changed>(&failed),
        Err(EventDecodeError::UnsuccessfulCall)
    ));
    let mut rpc = rpc_event(&raw);
    rpc.in_successful_contract_call = Some(false);
    assert!(matches!(
        spec.decode_event::<Changed>(&rpc),
        Err(EventDecodeError::UnsuccessfulCall)
    ));
    let mut rpc = rpc_event(&raw);
    rpc.contract_id = ACCOUNT.into();
    assert!(matches!(
        spec.decode_event::<Changed>(&rpc),
        Err(EventDecodeError::InvalidContractId)
    ));
    let mut json = serde_json::json!({
        "type":"contract","contractId":CONTRACT,"ledger":100,"ledgerClosedAt":"today","id":"1-1","txHash":"test","topic":["bad"],"value":ScVal::U32(7).to_xdr_base64(Limits::none()).unwrap()
    });
    let malformed: EventResponse = serde_json::from_value(json.clone()).unwrap();
    assert!(matches!(
        spec.decode_event::<Changed>(&malformed),
        Err(EventDecodeError::Rpc(_))
    ));
    json["topic"] = serde_json::json!([symbol("changed").to_xdr_base64(Limits::none()).unwrap()]);
    json["value"] = serde_json::json!("bad");
    let malformed: EventResponse = serde_json::from_value(json).unwrap();
    assert!(matches!(
        spec.decode_event::<Changed>(&malformed),
        Err(EventDecodeError::Rpc(_))
    ));
}
