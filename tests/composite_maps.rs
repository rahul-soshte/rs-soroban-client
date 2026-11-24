#![cfg(feature = "contract-spec")]
mod support;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use soroban_client::{
    contract_spec::{ContractSpec, I256, U256},
    xdr::*,
};
use support::*;

fn map(key_type: ScSpecTypeDef, value_type: ScSpecTypeDef) -> ScSpecTypeDef {
    ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(key_type),
        value_type: Box::new(value_type),
    }))
}

fn tuple(types: Vec<ScSpecTypeDef>) -> ScSpecTypeDef {
    ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
        value_types: types.try_into().unwrap(),
    }))
}

fn strukt(name: &str, fields: &[(&str, ScSpecTypeDef)]) -> ScSpecEntry {
    ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
        name: name.try_into().unwrap(),
        fields: fields
            .iter()
            .map(|(name, ty)| ScSpecUdtStructFieldV0 {
                name: (*name).try_into().unwrap(),
                type_: ty.clone(),
                doc: Default::default(),
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        ..Default::default()
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
struct Key {
    z: u32,
    a: u32,
}

fn spec() -> ContractSpec {
    use ScSpecTypeDef as T;
    let mut entries = entries();
    entries.push(strukt("Key", &[("z", T::U32), ("a", T::U32)]));
    let values = map(
        udt("Key"),
        T::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(T::Option(Box::new(ScSpecTypeOption {
                value_type: Box::new(T::U128),
            }))),
        })),
    );
    entries.push(strukt(
        "Envelope",
        &[
            ("values", values.clone()),
            (
                "note",
                T::Option(Box::new(ScSpecTypeOption {
                    value_type: Box::new(T::String),
                })),
            ),
        ],
    ));
    entries.push(strukt("Wrapped", &[("0", values.clone())]));
    entries.push(ScSpecEntry::UdtUnionV0(ScSpecUdtUnionV0 {
        name: "Batch".try_into().unwrap(),
        cases: vec![
            ScSpecUdtUnionCaseV0::VoidV0(ScSpecUdtUnionCaseVoidV0 {
                name: "Empty".try_into().unwrap(),
                ..Default::default()
            }),
            ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                name: "Values".try_into().unwrap(),
                type_: vec![values].try_into().unwrap(),
                ..Default::default()
            }),
            ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                name: "Pair".try_into().unwrap(),
                type_: vec![udt("Envelope"), T::U32].try_into().unwrap(),
                ..Default::default()
            }),
        ]
        .try_into()
        .unwrap(),
        ..Default::default()
    }));
    ContractSpec::new(entries).unwrap()
}

fn roundtrip<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: T,
    ty: ScSpecTypeDef,
) {
    let spec = spec();
    let wire = spec.encode(&value, &ty).unwrap();
    assert_eq!(spec.decode::<T>(&wire, &ty).unwrap(), value);
}

#[test]
fn tuple_keys_work_with_btree_and_hash_maps() {
    use ScSpecTypeDef as T;
    let spec = spec();
    let ty = map(tuple(vec![T::U32, T::U32]), T::I128);
    let values = BTreeMap::from([((9u32, 0u32), i128::MAX), ((1, 2), i128::MIN), ((1, 1), 0)]);
    let hashed: HashMap<_, _> = values.clone().into_iter().collect();
    let wire = spec.encode(&values, &ty).unwrap();
    assert_eq!(wire, spec.encode(&hashed, &ty).unwrap());
    assert_eq!(
        spec.decode::<BTreeMap<(u32, u32), i128>>(&wire, &ty)
            .unwrap(),
        values
    );
    assert_eq!(
        spec.decode::<HashMap<(u32, u32), i128>>(&wire, &ty)
            .unwrap(),
        hashed
    );
    let ScVal::Map(Some(entries)) = wire else {
        panic!()
    };
    assert!(entries.windows(2).all(|p| p[0].key < p[1].key));
    roundtrip(BTreeMap::<(u32, u32), i128>::new(), ty);
}

#[test]
fn struct_keys_use_wire_order_not_rust_field_order() {
    use ScSpecTypeDef as T;
    let spec = spec();
    let first = Key { z: 0, a: 9 };
    let second = Key { z: 1, a: 0 };
    assert!(first < second);
    let values = BTreeMap::from([(first.clone(), U256::MAX), (second.clone(), U256::ONE)]);
    let ty = map(udt("Key"), T::U256);
    let wire = spec.encode(&values, &ty).unwrap();
    let ScVal::Map(Some(entries)) = &wire else {
        panic!()
    };
    assert_eq!(entries[0].key, spec.encode(&second, &udt("Key")).unwrap());
    assert_eq!(entries[1].key, spec.encode(&first, &udt("Key")).unwrap());
    assert_eq!(
        spec.decode::<BTreeMap<Key, U256>>(&wire, &ty).unwrap(),
        values
    );
    let hashed: HashMap<_, _> = values.into_iter().collect();
    assert_eq!(spec.encode(&hashed, &ty).unwrap(), wire);
    assert_eq!(
        spec.decode::<HashMap<Key, U256>>(&wire, &ty).unwrap(),
        hashed
    );
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Envelope {
    values: BTreeMap<Key, Vec<Option<u128>>>,
    note: Option<String>,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Wrapped(BTreeMap<Key, Vec<Option<u128>>>);
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Batch {
    Empty,
    Values(BTreeMap<Key, Vec<Option<u128>>>),
    Pair(Envelope, u32),
}

#[test]
fn maps_nested_in_structs_unions_options_and_containers() {
    use ScSpecTypeDef as T;
    let values = BTreeMap::from([(Key { z: 7, a: 3 }, vec![Some(u128::MAX), None, Some(0)])]);
    let envelope = Envelope {
        values: values.clone(),
        note: None,
    };
    roundtrip(envelope.clone(), udt("Envelope"));
    roundtrip(Wrapped(values.clone()), udt("Wrapped"));
    roundtrip(Batch::Empty, udt("Batch"));
    roundtrip(Batch::Values(values), udt("Batch"));
    roundtrip(Batch::Pair(envelope.clone(), 42), udt("Batch"));
    roundtrip(
        Some(envelope.clone()),
        T::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(udt("Envelope")),
        })),
    );
    roundtrip(
        None::<Envelope>,
        T::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(udt("Envelope")),
        })),
    );
    roundtrip(
        vec![envelope.clone()],
        T::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(udt("Envelope")),
        })),
    );
    roundtrip((envelope, 7u32), tuple(vec![udt("Envelope"), T::U32]));
}

#[test]
fn map_keys_can_themselves_contain_maps() {
    use ScSpecTypeDef as T;
    let inner = map(tuple(vec![T::U32, T::U32]), T::U128);
    roundtrip(
        BTreeMap::from([
            (BTreeMap::from([((1u32, 2u32), u128::MAX)]), 7u32),
            (BTreeMap::new(), 9u32),
        ]),
        map(inner, T::U32),
    );
}

#[test]
fn primitive_enum_and_union_keys_remain_native() {
    use ScSpecTypeDef as T;
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    enum Color {
        Red,
        Green,
        Blue,
    }
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    enum Choice {
        Empty,
        One(u128),
        Pair(u32, String),
    }
    roundtrip(
        BTreeMap::from([(Color::Blue, 11u32), (Color::Red, 3)]),
        map(udt("Color"), T::U32),
    );
    roundtrip(
        BTreeMap::from([
            (Choice::One(u128::MAX), 7u32),
            (Choice::Pair(2, "hi".into()), 8),
            (Choice::Empty, 9),
        ]),
        map(udt("Choice"), T::U32),
    );
    roundtrip(
        BTreeMap::from([(u128::MAX, I256::MIN), (0, I256::MAX)]),
        map(T::U128, T::I256),
    );
    roundtrip(
        HashMap::from([(I256::MIN, U256::MAX), (I256::ZERO, U256::ZERO)]),
        map(T::I256, T::U256),
    );
    roundtrip(
        BTreeMap::from([(None, 1u32), (Some((1u32, 2u32)), 2)]),
        map(
            T::Option(Box::new(ScSpecTypeOption {
                value_type: Box::new(tuple(vec![T::U32, T::U32])),
            })),
            T::U32,
        ),
    );
    roundtrip(
        BTreeMap::from([("hello".to_string(), vec![1u8, 2])]),
        map(T::String, T::Bytes),
    );
    let spec = spec();
    let ty = map(T::U32, T::U32);
    assert_eq!(
        spec.encode(&serde_json::json!({"7":42}), &ty).unwrap(),
        spec.encode(&BTreeMap::from([(7u32, 42u32)]), &ty).unwrap()
    );
}

#[test]
fn duplicate_keys_after_conversion_are_rejected() {
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
    enum Alias {
        #[serde(rename = "Red")]
        First,
        #[serde(rename = "Red")]
        Second,
    }
    let value = BTreeMap::from([(Alias::First, 1u32), (Alias::Second, 2)]);
    let error = spec()
        .encode(&value, &map(udt("Color"), ScSpecTypeDef::U32))
        .unwrap_err();
    assert!(error.to_string().contains("duplicate"));
}

#[test]
fn invalid_wire_maps_and_key_shapes_are_rejected() {
    use ScSpecTypeDef as T;
    let spec = spec();
    let ty = map(tuple(vec![T::U32, T::U32]), T::U32);
    let key = |a, b| ScVal::Vec(Some(vec![ScVal::U32(a), ScVal::U32(b)].try_into().unwrap()));
    for keys in [
        vec![key(2, 0), key(1, 0)],
        vec![key(1, 0), key(1, 0)],
        vec![ScVal::U32(1)],
    ] {
        let wire = ScVal::Map(Some(
            keys.into_iter()
                .map(|key| ScMapEntry {
                    key,
                    val: ScVal::U32(1),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        ));
        assert!(spec
            .decode::<BTreeMap<(u32, u32), u32>>(&wire, &ty)
            .is_err());
    }
    assert!(spec
        .encode(&BTreeMap::from([((1u32,), 2u32)]), &ty)
        .is_err());
    assert!(spec
        .encode(&BTreeMap::from([((u64::MAX, 0u32), 2u32)]), &ty)
        .is_err());
    assert!(spec.encode(&vec![((1u32, 2u32), 3u32)], &ty).is_err());
    let wrong_value = ScVal::Map(Some(
        vec![ScMapEntry {
            key: key(1, 2),
            val: ScVal::String(ScString("wrong".try_into().unwrap())),
        }]
        .try_into()
        .unwrap(),
    ));
    assert!(spec
        .decode::<BTreeMap<(u32, u32), u32>>(&wrong_value, &ty)
        .is_err());
    let wide = spec
        .encode(&BTreeMap::from([((1u32, 2u32), u32::MAX)]), &ty)
        .unwrap();
    assert!(spec.decode::<BTreeMap<(u32, u32), u8>>(&wide, &ty).is_err());
}

#[test]
fn named_and_positional_arguments_accept_maps_without_wrappers() {
    use ScSpecTypeDef as T;
    let ty = map(tuple(vec![T::U32, T::U32]), T::U32);
    let mut entries = entries();
    entries.push(function("process", &[("values", ty.clone())], ty.clone()));
    let spec = ContractSpec::new(entries).unwrap();
    let values = BTreeMap::from([((1u32, 2u32), 3u32)]);
    #[derive(Serialize)]
    struct Args<'a> {
        values: &'a BTreeMap<(u32, u32), u32>,
    }
    let expected = vec![spec.encode(&values, &ty).unwrap()];
    assert_eq!(spec.encode_args("process", &(&values,)).unwrap(), expected);
    assert_eq!(
        spec.encode_args("process", &Args { values: &values })
            .unwrap(),
        expected
    );
    assert_eq!(
        spec.decode_result::<HashMap<(u32, u32), u32>>("process", &expected[0])
            .unwrap(),
        values.into_iter().collect()
    );
}
