use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Failure {
    Overflow,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultRecord {
    pub value: Result<u32, Failure>,
    pub values: Vec<Result<u32, Failure>>,
    pub keyed: BTreeMap<u32, Result<u32, Failure>>,
    pub optional: Option<Result<u32, Failure>>,
}

pub fn record() -> ResultRecord {
    ResultRecord {
        value: Ok(42),
        values: vec![Ok(42), Err(Failure::Overflow)],
        keyed: BTreeMap::from([(1, Ok(42)), (2, Err(Failure::Overflow))]),
        optional: Some(Err(Failure::Overflow)),
    }
}

pub fn result_type() -> ScSpecTypeDef {
    result_with(udt("ArithmeticFailure"))
}

pub fn result_with(error: ScSpecTypeDef) -> ScSpecTypeDef {
    ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
        ok_type: Box::new(ScSpecTypeDef::U32),
        error_type: Box::new(error),
    }))
}

pub fn vec_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
        element_type: Box::new(result_type()),
    }))
}

pub fn map_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(ScSpecTypeDef::U32),
        value_type: Box::new(result_type()),
    }))
}

pub fn option_type() -> ScSpecTypeDef {
    ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
        value_type: Box::new(result_type()),
    }))
}

pub fn vector() -> ScVal {
    ScVal::Vec(Some(
        vec![ScVal::U32(42), ScVal::Error(ScError::Contract(7))]
            .try_into()
            .unwrap(),
    ))
}

pub fn map() -> ScVal {
    ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            [
                (ScVal::U32(1), ScVal::U32(42)),
                (ScVal::U32(2), ScVal::Error(ScError::Contract(7))),
            ]
            .into_iter(),
        )
        .unwrap(),
    ))
}

pub fn record_value() -> ScVal {
    ScVal::Map(Some(
        ScMap::sorted_from_pairs(
            [
                ("value", ScVal::U32(42)),
                ("values", vector()),
                ("keyed", map()),
                ("optional", ScVal::Error(ScError::Contract(7))),
            ]
            .into_iter()
            .map(|(key, val)| (ScVal::Symbol(key.try_into().unwrap()), val)),
        )
        .unwrap(),
    ))
}

pub fn entries() -> Vec<ScSpecEntry> {
    let event = |name: &str, prefix: &str, ty| {
        ScSpecEntry::EventV0(ScSpecEventV0 {
            name: name.try_into().unwrap(),
            prefix_topics: vec![prefix.try_into().unwrap()].try_into().unwrap(),
            params: vec![ScSpecEventParamV0 {
                name: "value".try_into().unwrap(),
                type_: ty,
                location: ScSpecEventParamLocationV0::Data,
                doc: Default::default(),
            }]
            .try_into()
            .unwrap(),
            data_format: ScSpecEventDataFormat::SingleValue,
            ..Default::default()
        })
    };
    vec![
        ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            name: "ResultRecord".try_into().unwrap(),
            fields: [
                ("value", result_type()),
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
        }),
        function(
            "accept_result",
            &[("value", result_type())],
            ScSpecTypeDef::U32,
        ),
        function(
            "accept_any_result",
            &[("value", result_with(ScSpecTypeDef::Error))],
            ScSpecTypeDef::U32,
        ),
        function("echo_results", &[("value", vec_type())], vec_type()),
        function(
            "echo_result_record",
            &[("value", udt("ResultRecord"))],
            udt("ResultRecord"),
        ),
        function("result_record", &[], udt("ResultRecord")),
        function(
            "publish_result",
            &[("value", result_type())],
            ScSpecTypeDef::Void,
        ),
        event("ResultDatum", "result_datum", result_type()),
        event("ResultReport", "result_report", udt("ResultRecord")),
    ]
}
