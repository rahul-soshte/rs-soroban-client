#![allow(dead_code)]
use soroban_client::xdr::*;

pub const CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
pub const ACCOUNT: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

pub fn udt(name: &str) -> ScSpecTypeDef {
    ScSpecTypeDef::Udt(ScSpecTypeUdt {
        name: name.try_into().unwrap(),
    })
}

pub fn function(
    name: &str,
    inputs: &[(&str, ScSpecTypeDef)],
    output: ScSpecTypeDef,
) -> ScSpecEntry {
    ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
        doc: "Function documentation".try_into().unwrap(),
        name: name.try_into().unwrap(),
        inputs: inputs
            .iter()
            .map(|(name, ty)| ScSpecFunctionInputV0 {
                doc: Default::default(),
                name: (*name).try_into().unwrap(),
                type_: ty.clone(),
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        outputs: vec![output].try_into().unwrap(),
    })
}

pub fn entries() -> Vec<ScSpecEntry> {
    use ScSpecTypeDef as T;
    vec![
        function(
            "add",
            &[("a", T::U32), ("b", T::U32)],
            T::Result(Box::new(ScSpecTypeResult {
                ok_type: Box::new(T::U32),
                error_type: Box::new(T::Error),
            })),
        ),
        function("echo", &[("record", udt("Record"))], udt("Record")),
        ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            name: "Record".try_into().unwrap(),
            fields: vec![
                ScSpecUdtStructFieldV0 {
                    name: "z".try_into().unwrap(),
                    type_: T::I128,
                    doc: Default::default(),
                },
                ScSpecUdtStructFieldV0 {
                    name: "a".try_into().unwrap(),
                    type_: T::String,
                    doc: Default::default(),
                },
            ]
            .try_into()
            .unwrap(),
            ..Default::default()
        }),
        ScSpecEntry::UdtUnionV0(ScSpecUdtUnionV0 {
            name: "Choice".try_into().unwrap(),
            cases: vec![
                ScSpecUdtUnionCaseV0::VoidV0(ScSpecUdtUnionCaseVoidV0 {
                    name: "Empty".try_into().unwrap(),
                    doc: Default::default(),
                }),
                ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                    name: "One".try_into().unwrap(),
                    doc: Default::default(),
                    type_: vec![T::U128].try_into().unwrap(),
                }),
                ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                    name: "Pair".try_into().unwrap(),
                    doc: Default::default(),
                    type_: vec![T::U32, T::String].try_into().unwrap(),
                }),
            ]
            .try_into()
            .unwrap(),
            ..Default::default()
        }),
        ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
            name: "Color".try_into().unwrap(),
            cases: [("Red", 3), ("Green", 7), ("Blue", 11)]
                .into_iter()
                .map(|(name, value)| ScSpecUdtEnumCaseV0 {
                    name: name.try_into().unwrap(),
                    value,
                    doc: Default::default(),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            ..Default::default()
        }),
        ScSpecEntry::UdtErrorEnumV0(ScSpecUdtErrorEnumV0 {
            name: "ArithmeticFailure".try_into().unwrap(),
            cases: vec![ScSpecUdtErrorEnumCaseV0 {
                name: "Overflow".try_into().unwrap(),
                value: 7,
                doc: "Too large".try_into().unwrap(),
            }]
            .try_into()
            .unwrap(),
            ..Default::default()
        }),
        ScSpecEntry::EventV0(ScSpecEventV0 {
            name: "Changed".try_into().unwrap(),
            ..Default::default()
        }),
        function("echo_map", &[("value", map_type())], map_type()),
    ]
}

pub fn map_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
            value_types: vec![ScSpecTypeDef::U32, ScSpecTypeDef::U32]
                .try_into()
                .unwrap(),
        }))),
        value_type: Box::new(ScSpecTypeDef::I128),
    }))
}

pub fn meta(key: &str, value: &str) -> Vec<u8> {
    ScMetaEntry::ScMetaV0(ScMetaV0 {
        key: key.try_into().unwrap(),
        val: value.try_into().unwrap(),
    })
    .to_xdr(Limits::none())
    .unwrap()
}

fn leb(mut value: usize, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        out.push(byte | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}

pub fn wasm(sections: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"\0asm\x01\0\0\0".to_vec();
    for (name, data) in sections {
        let mut section = Vec::new();
        leb(name.len(), &mut section);
        section.extend_from_slice(name.as_bytes());
        section.extend_from_slice(data);
        out.push(0);
        leb(section.len(), &mut out);
        out.extend(section);
    }
    out
}

pub fn interface_wasm() -> Vec<u8> {
    let spec = entries()
        .iter()
        .flat_map(|e| e.to_xdr(Limits::none()).unwrap())
        .collect();
    wasm(&[
        ("contractspecv0", spec),
        ("contractmetav0", meta("sep", "41,40")),
        ("contractmetav0", meta("sep", "48,41")),
    ])
}
