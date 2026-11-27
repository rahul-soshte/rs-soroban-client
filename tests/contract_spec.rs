#![cfg(feature = "contract-spec")]
mod support;

use serde::{Deserialize, Serialize};
use soroban_client::{contract_spec::*, xdr::*};
use support::*;

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
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Color {
    Red,
    Green,
    Blue,
}

fn roundtrip<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: T,
    ty: ScSpecTypeDef,
) {
    let spec = ContractSpec::new(entries()).unwrap();
    let encoded = spec.encode(&value, &ty).unwrap();
    let decoded: T = spec.decode(&encoded, &ty).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn native_udts_follow_sep48_wire_format() {
    let spec = ContractSpec::new(entries()).unwrap();
    let record = Record {
        z: i128::MIN,
        a: "hello".into(),
    };
    let wire = spec.encode(&record, &udt("Record")).unwrap();
    let ScVal::Map(Some(map)) = &wire else {
        panic!("not a struct map")
    };
    assert_eq!(map[0].key, ScVal::Symbol("a".try_into().unwrap()));
    assert_eq!(map[1].key, ScVal::Symbol("z".try_into().unwrap()));
    assert_eq!(
        spec.decode::<Record>(&wire, &udt("Record")).unwrap(),
        record
    );
    for value in [
        Choice::Empty,
        Choice::One(u128::MAX),
        Choice::Pair(9, "hi".into()),
    ] {
        roundtrip(value, udt("Choice"));
    }
    assert_eq!(
        spec.encode(&Color::Green, &udt("Color")).unwrap(),
        ScVal::U32(7)
    );
    for value in [Color::Red, Color::Green, Color::Blue] {
        roundtrip(value, udt("Color"));
    }
}

#[test]
fn full_width_integer_boundaries() {
    use ScSpecTypeDef as T;
    for value in [0, 1, u128::MAX, u64::MAX as u128 + 1] {
        roundtrip(value, T::U128);
    }
    for value in [i128::MIN, -1, 0, i128::MAX] {
        roundtrip(value, T::I128);
    }
    for value in [U256::ZERO, U256::ONE, U256::MAX] {
        roundtrip(value, T::U256);
    }
    for value in [I256::MIN, I256::new(-1), I256::ZERO, I256::MAX] {
        roundtrip(value, T::I256);
    }
    roundtrip(
        vec![u128::MAX, 0],
        T::Vec(Box::new(ScSpecTypeVec {
            element_type: Box::new(T::U128),
        })),
    );
    assert!(ContractSpec::new(entries())
        .unwrap()
        .encode(&"340282366920938463463374607431768211456", &T::U128)
        .is_err());
}

#[test]
fn containers_bytes_unit_time_and_options() {
    use ScSpecTypeDef as T;
    roundtrip((), T::Void);
    roundtrip(u64::MAX, T::Timepoint);
    roundtrip(123u64, T::Duration);
    roundtrip(vec![0u8, 128, 255], T::Bytes);
    roundtrip([7u8; 4], T::BytesN(ScSpecTypeBytesN { n: 4 }));
    roundtrip(
        Some(u128::MAX),
        T::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(T::U128),
        })),
    );
    roundtrip(
        None::<u32>,
        T::Option(Box::new(ScSpecTypeOption {
            value_type: Box::new(T::U32),
        })),
    );
    roundtrip(
        std::collections::BTreeMap::from([(7u32, u128::MAX)]),
        T::Map(Box::new(ScSpecTypeMap {
            key_type: Box::new(T::U32),
            value_type: Box::new(T::U128),
        })),
    );
}

#[test]
fn named_and_positional_arguments() {
    let spec = ContractSpec::new(entries()).unwrap();
    #[derive(Serialize)]
    struct Args {
        b: u32,
        a: u32,
    }
    assert_eq!(
        spec.encode_args("add", &(2u32, 3u32)).unwrap(),
        vec![ScVal::U32(2), ScVal::U32(3)]
    );
    assert_eq!(
        spec.encode_args("add", &Args { b: 3, a: 2 }).unwrap(),
        vec![ScVal::U32(2), ScVal::U32(3)]
    );
    assert!(spec.encode_args("add", &(2u32,)).is_err());
    assert!(spec.encode_args("missing", &()).is_err());
    assert!(spec.encode_args("add", &(u64::MAX, 0u32)).is_err());
    assert!(spec
        .encode_args("add", &serde_json::json!({"a":2,"wrong":3}))
        .is_err());
    assert_eq!(spec.decode_result::<u32>("add", &ScVal::U32(5)).unwrap(), 5);
}

#[test]
fn rejects_malformed_values_without_unwinding() {
    let spec = ContractSpec::new(entries()).unwrap();
    assert!(spec
        .encode(&serde_json::json!({"One":null}), &udt("Choice"))
        .is_err());
    assert!(spec
        .encode(&serde_json::json!({"Pair":[1]}), &udt("Choice"))
        .is_err());
    assert!(spec
        .encode(&serde_json::json!({"Empty":123}), &udt("Choice"))
        .is_err());
    assert!(spec
        .encode(&[1u8, 2], &ScSpecTypeDef::BytesN(ScSpecTypeBytesN { n: 4 }))
        .is_err());
    assert!(spec
        .decode::<Color>(&ScVal::U32(99), &udt("Color"))
        .is_err());
    assert!(spec
        .decode::<Record>(
            &ScVal::Map(Some(Vec::new().try_into().unwrap())),
            &udt("Record")
        )
        .is_err());
    assert!(spec
        .decode::<Choice>(
            &ScVal::Vec(Some(Vec::new().try_into().unwrap())),
            &udt("Choice")
        )
        .is_err());
    assert!(ContractSpec::from_wasm(b"not wasm").is_err());
    assert!(ContractSpec::from_wasm(&wasm(&[])).is_err());
}

#[test]
fn reads_complete_interface_and_metadata() {
    let bytes = interface_wasm();
    let spec = ContractSpec::from_wasm(&bytes).unwrap();
    assert_eq!(spec.entries(), entries());
    assert!(spec
        .entries()
        .iter()
        .any(|e| matches!(e, ScSpecEntry::EventV0(_))));
    assert_eq!(
        spec.function("add").unwrap().doc.to_utf8_string_lossy(),
        "Function documentation"
    );
    let metadata = ContractMetadata::from_wasm(&bytes).unwrap();
    assert_eq!(metadata.entries.len(), 2);
    assert_eq!(metadata.sep_claims().unwrap(), vec![41, 40, 48]);
    assert!(ContractMetadata::from_wasm(&wasm(&[]))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn metadata_streams_do_not_span_sections() {
    let mut first = meta("sdk", "version");
    first.extend(meta("sep", "41"));
    let bytes = wasm(&[
        ("contractmetav0", first.clone()),
        ("contractmetav0", meta("sep", "48")),
    ]);
    assert_eq!(
        ContractMetadata::from_wasm(&bytes).unwrap().entries.len(),
        3
    );
    let split = first.len() - 2;
    let broken = wasm(&[
        ("contractmetav0", first[..split].to_vec()),
        ("contractmetav0", first[split..].to_vec()),
    ]);
    assert!(ContractMetadata::from_wasm(&broken).is_err());
    let mut trailing = meta("sep", "41");
    trailing.push(0);
    assert!(ContractMetadata::from_wasm(&wasm(&[("contractmetav0", trailing)])).is_err());
}

#[test]
fn malformed_sep_claims_are_not_silently_accepted() {
    for value in ["", "41,", "41, 48", "-1", "4294967296", "token"] {
        let metadata = ContractMetadata {
            entries: vec![MetadataEntry {
                key: "sep".into(),
                value: value.into(),
            }],
        };
        assert!(metadata.sep_claims().is_err(), "{value:?}");
    }
    let metadata = ContractMetadata {
        entries: vec![MetadataEntry {
            key: "sep".into(),
            value: "0041,41".into(),
        }],
    };
    assert_eq!(metadata.sep_claims().unwrap(), vec![41]);
}

#[test]
fn tuple_structs_and_nested_large_values() {
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Newtype(u128);
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Pair(Newtype, I256);
    let mut all = entries();
    for (name, types) in [
        ("Newtype", vec![ScSpecTypeDef::U128]),
        ("Pair", vec![udt("Newtype"), ScSpecTypeDef::I256]),
    ] {
        all.push(ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            name: name.try_into().unwrap(),
            fields: types
                .into_iter()
                .enumerate()
                .map(|(i, type_)| ScSpecUdtStructFieldV0 {
                    name: i.to_string().try_into().unwrap(),
                    type_,
                    doc: Default::default(),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            ..Default::default()
        }));
    }
    let spec = ContractSpec::new(all).unwrap();
    let value = Pair(Newtype(u128::MAX), I256::MIN);
    let wire = spec.encode(&value, &udt("Pair")).unwrap();
    assert_eq!(spec.decode::<Pair>(&wire, &udt("Pair")).unwrap(), value);
}

#[test]
fn rejects_duplicate_spec_sections_and_muxed_plain_addresses() {
    let data: Vec<u8> = entries()
        .iter()
        .flat_map(|e| e.to_xdr(Limits::none()).unwrap())
        .collect();
    assert!(ContractSpec::from_wasm(&wasm(&[
        ("contractspecv0", data.clone()),
        ("contractspecv0", data)
    ]))
    .is_err());
    let spec = ContractSpec::new(entries()).unwrap();
    let muxed = "MAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB5IG";
    assert!(spec.encode(&muxed, &ScSpecTypeDef::Address).is_err());
    roundtrip(muxed.to_string(), ScSpecTypeDef::MuxedAddress);
}
