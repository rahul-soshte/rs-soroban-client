use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitRecord {
    pub value: (),
}

pub fn tuple_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
        value_types: Default::default(),
    }))
}

pub fn record_value() -> ScVal {
    ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            [(ScVal::Symbol("value".try_into().unwrap()), ScVal::Void)].into_iter(),
        )
        .unwrap(),
    ))
}

pub fn entries() -> Vec<ScSpecEntry> {
    vec![
        function("echo_unit", &[("value", tuple_type())], tuple_type()),
        function("unit_output", &[], tuple_type()),
        function("unit_record", &[], udt("UnitRecord")),
        function("publish_unit", &[("value", tuple_type())], tuple_type()),
        ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            name: "UnitRecord".try_into().unwrap(),
            fields: vec![ScSpecUdtStructFieldV0 {
                name: "value".try_into().unwrap(),
                type_: tuple_type(),
                doc: Default::default(),
            }]
            .try_into()
            .unwrap(),
            ..Default::default()
        }),
        ScSpecEntry::EventV0(ScSpecEventV0 {
            name: "UnitDatum".try_into().unwrap(),
            prefix_topics: vec!["unit_datum".try_into().unwrap()].try_into().unwrap(),
            params: vec![ScSpecEventParamV0 {
                name: "value".try_into().unwrap(),
                type_: tuple_type(),
                location: ScSpecEventParamLocationV0::Data,
                doc: Default::default(),
            }]
            .try_into()
            .unwrap(),
            data_format: ScSpecEventDataFormat::SingleValue,
            ..Default::default()
        }),
    ]
}
