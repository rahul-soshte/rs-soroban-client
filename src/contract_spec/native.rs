//! Preserve native maps, results, errors and explicit ScVal tags at the Serde boundary.
//! Other subtrees are delegated to upstream JSON conversion.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use serde_content::{Data, Number};
use stellar_baselib::xdr::{ScError, ScMap, ScMapEntry, ScSpecTypeDef as T, Validate};

use super::{conversion, ContractSpec, ScSpecEntry, ScSpecUdtUnionCaseV0, ScVal, SpecError};

pub(super) type Value = serde_content::Value<'static>;

pub(super) fn serialize<T: Serialize>(value: &T) -> Result<Value, SpecError> {
    let value = serde_content::Serializer::new()
        .human_readable()
        .serialize(value)
        .map_err(|e| conversion(e.to_string()))?;
    simplify(value, 0)
}

fn nesting(depth: usize) -> Result<(), SpecError> {
    if depth > 100 {
        Err(conversion("value nesting exceeds 100"))
    } else {
        Ok(())
    }
}

// Match the externally tagged, human-readable conventions of the existing API,
// but retain maps as pairs instead of coercing their keys into JSON strings.
pub(super) fn simplify(value: Value, depth: usize) -> Result<Value, SpecError> {
    nesting(depth)?;
    let recurse = |v| simplify(v, depth + 1);
    Ok(match value {
        Value::Struct(s) => {
            // serde_json's arbitrary-precision Number uses a private newtype-like
            // struct when serialized to a non-JSON serializer.
            if s.name == "$serde_json::private::Number" {
                let Data::Struct { mut fields } = s.data else {
                    return Err(conversion("invalid JSON number"));
                };
                if fields.len() != 1 {
                    return Err(conversion("invalid JSON number"));
                }
                let (_, Value::String(text)) = fields.remove(0) else {
                    return Err(conversion("invalid JSON number"));
                };
                number(&text)?
            } else {
                recurse(match s.data {
                    Data::Unit => Value::Unit,
                    Data::NewType { value } => value,
                    Data::Tuple { values } => Value::Seq(values),
                    Data::Struct { fields } => Value::Map(
                        fields
                            .into_iter()
                            .map(|(k, v)| (Value::String(k), v))
                            .collect(),
                    ),
                })?
            }
        }
        Value::Enum(e) => match e.data {
            Data::Unit => Value::String(e.variant),
            Data::NewType { value } => {
                Value::Map(vec![(Value::String(e.variant), recurse(value)?)])
            }
            Data::Tuple { values } => Value::Map(vec![(
                Value::String(e.variant),
                recurse(Value::Seq(values))?,
            )]),
            Data::Struct { .. } => {
                return Err(conversion(
                    "union struct variants are unsupported; use a named struct payload",
                ))
            }
        },
        Value::Tuple(v) | Value::Seq(v) => {
            Value::Seq(v.into_iter().map(recurse).collect::<Result<_, _>>()?)
        }
        Value::Map(v) => Value::Map(
            v.into_iter()
                .map(|(k, v)| Ok((recurse(k)?, recurse(v)?)))
                .collect::<Result<_, SpecError>>()?,
        ),
        Value::Option(Some(v)) => recurse(*v)?,
        Value::Option(None) => Value::Unit,
        Value::Bytes(v) => Value::Seq(v.iter().map(|b| Value::from(*b)).collect()),
        Value::Char(v) => Value::String(v.to_string().into()),
        other => other,
    })
}

fn number(text: &str) -> Result<Value, SpecError> {
    if let Ok(n) = text.parse::<i128>() {
        return Ok(Value::from(n));
    }
    if let Ok(n) = text.parse::<u128>() {
        return Ok(Value::from(n));
    }
    Err(conversion(
        "expected an integer representable by i128 or u128",
    ))
}

pub(super) fn named_fields(
    values: Vec<(Value, Value)>,
) -> Result<BTreeMap<String, Value>, SpecError> {
    let mut fields = BTreeMap::new();
    for (key, value) in values {
        let Value::String(name) = key else {
            return Err(conversion("expected named fields"));
        };
        if fields.insert(name.into_owned(), value).is_some() {
            return Err(conversion("duplicate field name"));
        }
    }
    Ok(fields)
}

fn to_json(value: Value, depth: usize) -> Result<serde_json::Value, SpecError> {
    use serde_json::Value as J;
    nesting(depth)?;
    Ok(match value {
        Value::Unit => J::Null,
        Value::Bool(v) => J::Bool(v),
        Value::String(v) => J::String(v.into_owned()),
        Value::Number(n) => match n {
            Number::I8(n) => J::from(n),
            Number::U8(n) => J::from(n),
            Number::I16(n) => J::from(n),
            Number::U16(n) => J::from(n),
            Number::I32(n) => J::from(n),
            Number::U32(n) => J::from(n),
            Number::I64(n) => J::from(n),
            Number::U64(n) => J::from(n),
            Number::I128(n) => serde_json::to_value(n).map_err(|e| conversion(e.to_string()))?,
            Number::U128(n) => serde_json::to_value(n).map_err(|e| conversion(e.to_string()))?,
            _ => return Err(conversion("floating-point contract values are unsupported")),
        },
        Value::Seq(v) => J::Array(
            v.into_iter()
                .map(|v| to_json(v, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Value::Map(v) => J::Object(
            named_fields(v)?
                .into_iter()
                .map(|(k, v)| Ok((k, to_json(v, depth + 1)?)))
                .collect::<Result<_, SpecError>>()?,
        ),
        _ => return Err(conversion("unexpected native representation")),
    })
}

fn from_json(value: serde_json::Value) -> Result<Value, SpecError> {
    use serde_json::Value as J;
    Ok(match value {
        J::Null => Value::Unit,
        J::Bool(v) => Value::Bool(v),
        J::Number(n) => number(&n.to_string())?,
        J::String(v) => Value::String(v.into()),
        J::Array(v) => Value::Seq(v.into_iter().map(from_json).collect::<Result<_, _>>()?),
        J::Object(v) => Value::Map(
            v.into_iter()
                .map(|(k, v)| Ok((Value::String(k.into()), from_json(v)?)))
                .collect::<Result<_, SpecError>>()?,
        ),
    })
}

fn wire_vec(values: Vec<ScVal>) -> Result<ScVal, SpecError> {
    Ok(ScVal::Vec(Some(values.try_into().map_err(
        |e: stellar_baselib::xdr::Error| conversion(e.to_string()),
    )?)))
}

fn wire_map(entries: Vec<ScMapEntry>) -> Result<ScVal, SpecError> {
    // This sorts by ScVal ordering and rejects duplicate encoded keys.
    Ok(ScVal::Map(Some(ScMap::sorted_from(entries).map_err(
        |e| conversion(format!("invalid map or duplicate encoded key: {e}")),
    )?)))
}

fn sequence(value: Value, expected: usize) -> Result<Vec<Value>, SpecError> {
    let Value::Seq(values) = value else {
        return Err(conversion("expected a tuple or vector"));
    };
    if values.len() != expected {
        return Err(conversion("container arity mismatch"));
    }
    Ok(values)
}

fn vector(value: &ScVal, expected: Option<usize>) -> Result<&[ScVal], SpecError> {
    let ScVal::Vec(Some(values)) = value else {
        return Err(conversion("expected a contract vector"));
    };
    if expected.is_some_and(|n| n != values.len()) {
        return Err(conversion("container arity mismatch"));
    }
    Ok(values.as_slice())
}

fn union(value: Value) -> Result<(String, Option<Value>), SpecError> {
    match value {
        Value::String(name) => Ok((name.into_owned(), None)),
        Value::Map(mut v) if v.len() == 1 => {
            let (Value::String(name), payload) = v.remove(0) else {
                return Err(conversion("expected a union variant name"));
            };
            Ok((name.into_owned(), Some(payload)))
        }
        _ => Err(conversion("expected an externally tagged union")),
    }
}

impl ContractSpec {
    // Shared-prefix event candidates routinely have different types. Reject
    // mismatches before upstream conversion, whose panic paths would abort an
    // application built with panic=abort rather than yield a conversion error.
    pub(super) fn check_wire_type(
        &self,
        value: &ScVal,
        ty: &T,
        depth: usize,
    ) -> Result<(), SpecError> {
        use stellar_baselib::xdr::ScAddress;
        nesting(depth)?;
        let check = |v, t: &T| self.check_wire_type(v, t, depth + 1);
        let mismatch = || conversion("value does not match its declared wire type");
        match (ty, value) {
            (T::Bool, ScVal::Bool(_))
            | (T::Void, ScVal::Void)
            | (T::Error, ScVal::Error(_))
            | (T::U32, ScVal::U32(_))
            | (T::I32, ScVal::I32(_))
            | (T::U64, ScVal::U64(_))
            | (T::I64, ScVal::I64(_))
            | (T::U128, ScVal::U128(_))
            | (T::I128, ScVal::I128(_))
            | (T::U256, ScVal::U256(_))
            | (T::I256, ScVal::I256(_))
            | (T::Timepoint, ScVal::Timepoint(_))
            | (T::Duration, ScVal::Duration(_))
            | (T::String, ScVal::String(_))
            | (T::Symbol, ScVal::Symbol(_))
            | (T::Bytes, ScVal::Bytes(_)) => Ok(()),
            (T::BytesN(t), ScVal::Bytes(bytes)) if bytes.len() == t.n as usize => Ok(()),
            (
                T::Address | T::MuxedAddress,
                ScVal::Address(ScAddress::Account(_) | ScAddress::Contract(_)),
            )
            | (
                T::MuxedAddress,
                ScVal::Address(ScAddress::MuxedAccount(_) | ScAddress::MuxedContract(_)),
            ) => Ok(()),
            (T::Option(_), ScVal::Void) => Ok(()),
            (T::Option(t), value) => check(value, &t.value_type),
            (T::Result(t), value @ ScVal::Error(_)) => check(value, &t.error_type),
            (T::Result(t), value) => check(value, &t.ok_type),
            (T::Vec(t), ScVal::Vec(Some(values))) => {
                values.iter().try_for_each(|v| check(v, &t.element_type))
            }
            (T::Tuple(t), ScVal::Void) if t.value_types.is_empty() => Ok(()),
            (T::Tuple(t), ScVal::Vec(Some(values)))
                if !t.value_types.is_empty() && values.len() == t.value_types.len() =>
            {
                values
                    .iter()
                    .zip(t.value_types.iter())
                    .try_for_each(|(v, t)| check(v, t))
            }
            (T::Map(t), ScVal::Map(Some(values))) => {
                values.iter().try_for_each(|v| {
                    check(&v.key, &t.key_type)?;
                    check(&v.val, &t.value_type)
                })?;
                values.validate().map_err(|e| conversion(e.to_string()))
            }
            (T::Udt(t), value) => match self.udt(&t.name.to_utf8_string_lossy())? {
                ScSpecEntry::UdtEnumV0(t) => {
                    if matches!(value,ScVal::U32(code) if t.cases.iter().any(|c| c.value == *code))
                    {
                        Ok(())
                    } else {
                        Err(mismatch())
                    }
                }
                ScSpecEntry::UdtErrorEnumV0(t) => {
                    if matches!(value, ScVal::Error(stellar_baselib::xdr::ScError::Contract(code)) if t.cases.iter().any(|c| c.value == *code))
                    {
                        Ok(())
                    } else {
                        Err(mismatch())
                    }
                }
                ScSpecEntry::UdtStructV0(t)
                    if t.fields.first().is_some_and(|f| f.name.as_slice() == b"0") =>
                {
                    vector(value, Some(t.fields.len()))?
                        .iter()
                        .zip(t.fields.iter())
                        .try_for_each(|(v, f)| check(v, &f.type_))
                }
                ScSpecEntry::UdtStructV0(t) => {
                    let ScVal::Map(Some(values)) = value else {
                        return Err(mismatch());
                    };
                    if values.len() != t.fields.len() {
                        return Err(mismatch());
                    }
                    t.fields.iter().try_for_each(|f| {
                        let entry = values.iter().find(|v| {
                            matches!(&v.key, ScVal::Symbol(name) if name.as_slice() == f.name.as_slice())
                        }).ok_or_else(mismatch)?;
                        check(&entry.val, &f.type_)
                    })?;
                    values.validate().map_err(|e| conversion(e.to_string()))
                }
                ScSpecEntry::UdtUnionV0(t) => {
                    let values = vector(value, None)?;
                    let Some(ScVal::Symbol(name)) = values.first() else {
                        return Err(mismatch());
                    };
                    let case = t
                        .cases
                        .iter()
                        .find(|c| match c {
                            ScSpecUdtUnionCaseV0::VoidV0(c) => c.name.as_slice() == name.as_slice(),
                            ScSpecUdtUnionCaseV0::TupleV0(c) => {
                                c.name.as_slice() == name.as_slice()
                            }
                        })
                        .ok_or_else(mismatch)?;
                    match case {
                        ScSpecUdtUnionCaseV0::VoidV0(_) if values.len() == 1 => Ok(()),
                        ScSpecUdtUnionCaseV0::TupleV0(c) if values.len() == c.type_.len() + 1 => {
                            values[1..]
                                .iter()
                                .zip(c.type_.iter())
                                .try_for_each(|(v, t)| check(v, t))
                        }
                        _ => Err(mismatch()),
                    }
                }
                _ => Err(mismatch()),
            },
            // Generic Val supports contract values, not internal ledger values.
            (T::Val, ScVal::Vec(Some(values))) => values.iter().try_for_each(|v| check(v, &T::Val)),
            (T::Val, ScVal::Map(Some(values))) => {
                values.iter().try_for_each(|v| {
                    check(&v.key, &T::Val)?;
                    check(&v.val, &T::Val)
                })?;
                values.validate().map_err(|e| conversion(e.to_string()))
            }
            (
                T::Val,
                ScVal::Bool(_)
                | ScVal::Void
                | ScVal::Error(_)
                | ScVal::U32(_)
                | ScVal::I32(_)
                | ScVal::U64(_)
                | ScVal::I64(_)
                | ScVal::U128(_)
                | ScVal::I128(_)
                | ScVal::U256(_)
                | ScVal::I256(_)
                | ScVal::String(_)
                | ScVal::Symbol(_)
                | ScVal::Bytes(_)
                | ScVal::Timepoint(_)
                | ScVal::Duration(_),
            ) => Ok(()),
            (
                T::Val,
                ScVal::Address(
                    ScAddress::Account(_)
                    | ScAddress::Contract(_)
                    | ScAddress::MuxedAccount(_)
                    | ScAddress::MuxedContract(_),
                ),
            ) => Ok(()),
            _ => Err(mismatch()),
        }
    }

    fn needs_native_bridge(
        &self,
        ty: &T,
        seen: &mut HashSet<String>,
        depth: usize,
    ) -> Result<bool, SpecError> {
        nesting(depth)?;
        Ok(match ty {
            T::Map(_) | T::Error | T::Result(_) | T::Val => true,
            T::Option(t) => self.needs_native_bridge(&t.value_type, seen, depth + 1)?,
            T::Vec(t) => self.needs_native_bridge(&t.element_type, seen, depth + 1)?,
            T::Tuple(t) => {
                let mut found = t.value_types.is_empty();
                for t in t.value_types.iter() {
                    found |= self.needs_native_bridge(t, seen, depth + 1)?;
                }
                found
            }
            T::Udt(t) => {
                let name = t.name.to_utf8_string_lossy();
                if !seen.insert(name.clone()) {
                    return Ok(false);
                }
                let found = match self.udt(&name)? {
                    ScSpecEntry::UdtStructV0(s) => {
                        let mut found = false;
                        for f in s.fields.iter() {
                            found |= self.needs_native_bridge(&f.type_, seen, depth + 1)?;
                        }
                        found
                    }
                    ScSpecEntry::UdtUnionV0(u) => {
                        let mut found = false;
                        for c in u.cases.iter() {
                            if let ScSpecUdtUnionCaseV0::TupleV0(c) = c {
                                for t in c.type_.iter() {
                                    found |= self.needs_native_bridge(t, seen, depth + 1)?;
                                }
                            }
                        }
                        found
                    }
                    ScSpecEntry::UdtErrorEnumV0(_) => true,
                    _ => false,
                };
                seen.remove(&name);
                found
            }
            _ => false,
        })
    }

    pub(super) fn encode_native(
        &self,
        value: Value,
        ty: &T,
        depth: usize,
    ) -> Result<ScVal, SpecError> {
        nesting(depth)?;
        if !self.needs_native_bridge(ty, &mut HashSet::new(), 0)? {
            return self.encode_json(to_json(value, depth)?, ty);
        }
        let encode = |v, t: &T| self.encode_native(v, t, depth + 1);
        match ty {
            T::Val => {
                // Use XDR's tagged Serde representation, not upstream's lossy
                // generic JSON renderer (which merges string/symbol, etc.).
                let value: ScVal = serde_json::from_value(to_json(value, depth)?)
                    .map_err(|e| conversion(format!("expected an explicit ScVal: {e}")))?;
                self.check_wire_type(&value, ty, depth)?;
                Ok(value)
            }
            T::Error => {
                let json = to_json(value, depth)?;
                // Preserve the existing explicit ScVal escape hatch as well as
                // accepting the human-readable ScError representation.
                if let Ok(ScVal::Error(error)) = serde_json::from_value::<ScVal>(json.clone()) {
                    return Ok(ScVal::Error(error));
                }
                serde_json::from_value::<ScError>(json)
                    .map(ScVal::Error)
                    .map_err(|e| conversion(format!("invalid error value: {e}")))
            }
            T::Map(t) => {
                let Value::Map(values) = value else {
                    return Err(conversion("expected a Rust map"));
                };
                wire_map(
                    values
                        .into_iter()
                        .map(|(k, v)| {
                            Ok(ScMapEntry {
                                key: self.encode_map_key(k, &t.key_type, depth + 1)?,
                                val: encode(v, &t.value_type)?,
                            })
                        })
                        .collect::<Result<_, SpecError>>()?,
                )
            }
            T::Option(t) => {
                if value == Value::Unit {
                    Ok(ScVal::Void)
                } else {
                    encode(value, &t.value_type)
                }
            }
            T::Result(t) => {
                let (name, payload) = union(value)?;
                let payload = payload.ok_or_else(|| conversion("expected a Result payload"))?;
                match name.as_str() {
                    "Ok" => {
                        let value = encode(payload, &t.ok_type)?;
                        if matches!(value, ScVal::Error(_)) {
                            return Err(conversion("Result::Ok must not encode as an error value"));
                        }
                        Ok(value)
                    }
                    "Err" => {
                        let error = encode(payload, &t.error_type)?;
                        if !matches!(error, ScVal::Error(_)) {
                            return Err(conversion("Result::Err must encode as an error value"));
                        }
                        Ok(error)
                    }
                    _ => Err(conversion("expected a Result Ok or Err variant")),
                }
            }
            T::Vec(t) => {
                let Value::Seq(values) = value else {
                    return Err(conversion("expected a vector"));
                };
                wire_vec(
                    values
                        .into_iter()
                        .map(|v| encode(v, &t.element_type))
                        .collect::<Result<_, _>>()?,
                )
            }
            T::Tuple(t) if t.value_types.is_empty() => {
                if value != Value::Unit {
                    return Err(conversion("expected unit for a zero-element tuple"));
                }
                Ok(ScVal::Void)
            }
            T::Tuple(t) => wire_vec(
                sequence(value, t.value_types.len())?
                    .into_iter()
                    .zip(t.value_types.iter())
                    .map(|(v, t)| encode(v, t))
                    .collect::<Result<_, _>>()?,
            ),
            T::Udt(t) => match self.udt(&t.name.to_utf8_string_lossy())? {
                ScSpecEntry::UdtErrorEnumV0(e) => {
                    let Value::String(name) = value else {
                        return Err(conversion("expected a declared error variant name"));
                    };
                    let case = e
                        .cases
                        .iter()
                        .find(|c| c.name.as_slice() == name.as_bytes())
                        .ok_or_else(|| conversion("unknown error enum variant"))?;
                    Ok(ScVal::Error(ScError::Contract(case.value)))
                }
                ScSpecEntry::UdtStructV0(s) => {
                    if s.fields.first().is_some_and(|f| f.name.as_slice() == b"0") {
                        let values = if s.fields.len() == 1 {
                            vec![value]
                        } else {
                            sequence(value, s.fields.len())?
                        };
                        wire_vec(
                            values
                                .into_iter()
                                .zip(s.fields.iter())
                                .map(|(v, f)| encode(v, &f.type_))
                                .collect::<Result<_, _>>()?,
                        )
                    } else {
                        let Value::Map(values) = value else {
                            return Err(conversion("expected a named struct"));
                        };
                        let mut fields = named_fields(values)?;
                        if fields.len() != s.fields.len() {
                            return Err(conversion("struct field count mismatch"));
                        }
                        wire_map(
                            s.fields
                                .iter()
                                .map(|f| {
                                    let name = f.name.to_utf8_string_lossy();
                                    let v = fields.remove(&name).ok_or_else(|| {
                                        conversion(format!("missing field {name}"))
                                    })?;
                                    Ok(ScMapEntry {
                                        key: self.encode_json(
                                            serde_json::Value::String(name),
                                            &T::Symbol,
                                        )?,
                                        val: encode(v, &f.type_)?,
                                    })
                                })
                                .collect::<Result<_, SpecError>>()?,
                        )
                    }
                }
                ScSpecEntry::UdtUnionV0(u) => {
                    let (name, payload) = union(value)?;
                    let case = u
                        .cases
                        .iter()
                        .find(|c| match c {
                            ScSpecUdtUnionCaseV0::VoidV0(c) => c.name.as_slice() == name.as_bytes(),
                            ScSpecUdtUnionCaseV0::TupleV0(c) => {
                                c.name.as_slice() == name.as_bytes()
                            }
                        })
                        .ok_or_else(|| conversion("unknown union variant"))?;
                    let mut values =
                        vec![self.encode_json(serde_json::Value::String(name), &T::Symbol)?];
                    match (case, payload) {
                        (ScSpecUdtUnionCaseV0::VoidV0(_), None) => (),
                        (ScSpecUdtUnionCaseV0::TupleV0(c), Some(payload)) => {
                            let payload = if c.type_.len() == 1 {
                                vec![payload]
                            } else {
                                sequence(payload, c.type_.len())?
                            };
                            values.extend(
                                payload
                                    .into_iter()
                                    .zip(c.type_.iter())
                                    .map(|(v, t)| encode(v, t))
                                    .collect::<Result<Vec<_>, _>>()?,
                            );
                        }
                        _ => return Err(conversion("union payload mismatch")),
                    }
                    wire_vec(values)
                }
                _ => Err(conversion("expected a container type")),
            },
            _ => Err(conversion("expected a container type")),
        }
    }

    pub(super) fn decode_native(
        &self,
        value: &ScVal,
        ty: &T,
        depth: usize,
    ) -> Result<Value, SpecError> {
        nesting(depth)?;
        if !self.needs_native_bridge(ty, &mut HashSet::new(), 0)? {
            return self.restore_options(from_json(self.decode_json(value, ty)?)?, ty, depth);
        }
        let decode = |v, t: &T| self.decode_native(v, t, depth + 1);
        match ty {
            T::Val => {
                from_json(serde_json::to_value(value).map_err(|e| conversion(e.to_string()))?)
            }
            T::Error => {
                let ScVal::Error(error) = value else {
                    return Err(conversion("expected an error value"));
                };
                from_json(serde_json::to_value(error).map_err(|e| conversion(e.to_string()))?)
            }
            T::Map(t) => {
                let ScVal::Map(Some(values)) = value else {
                    return Err(conversion("expected a contract map"));
                };
                values
                    .validate()
                    .map_err(|e| conversion(format!("invalid or noncanonical map: {e}")))?;
                Ok(Value::Map(
                    values
                        .iter()
                        .map(|e| Ok((decode(&e.key, &t.key_type)?, decode(&e.val, &t.value_type)?)))
                        .collect::<Result<_, SpecError>>()?,
                ))
            }
            T::Option(t) => Ok(Value::Option(if *value == ScVal::Void {
                None
            } else {
                Some(Box::new(decode(value, &t.value_type)?))
            })),
            T::Result(t) => {
                let (name, ty) = if matches!(value, ScVal::Error(_)) {
                    ("Err", &t.error_type)
                } else {
                    ("Ok", &t.ok_type)
                };
                Ok(Value::Map(vec![(
                    Value::String(name.into()),
                    decode(value, ty)?,
                )]))
            }
            T::Vec(t) => Ok(Value::Seq(
                vector(value, None)?
                    .iter()
                    .map(|v| decode(v, &t.element_type))
                    .collect::<Result<_, _>>()?,
            )),
            T::Tuple(t) if t.value_types.is_empty() => {
                if *value != ScVal::Void {
                    return Err(conversion("expected Void for a zero-element tuple"));
                }
                Ok(Value::Unit)
            }
            T::Tuple(t) => Ok(Value::Seq(
                vector(value, Some(t.value_types.len()))?
                    .iter()
                    .zip(t.value_types.iter())
                    .map(|(v, t)| decode(v, t))
                    .collect::<Result<_, _>>()?,
            )),
            T::Udt(t) => match self.udt(&t.name.to_utf8_string_lossy())? {
                ScSpecEntry::UdtErrorEnumV0(e) => {
                    let ScVal::Error(ScError::Contract(code)) = value else {
                        return Err(conversion("expected a contract error value"));
                    };
                    let case = e
                        .cases
                        .iter()
                        .find(|c| c.value == *code)
                        .ok_or_else(|| conversion("unknown error enum code"))?;
                    Ok(Value::String(case.name.to_utf8_string_lossy().into()))
                }
                ScSpecEntry::UdtStructV0(s) => {
                    if s.fields.first().is_some_and(|f| f.name.as_slice() == b"0") {
                        let mut values = vector(value, Some(s.fields.len()))?
                            .iter()
                            .zip(s.fields.iter())
                            .map(|(v, f)| decode(v, &f.type_))
                            .collect::<Result<Vec<_>, _>>()?;
                        if values.len() == 1 {
                            Ok(values.remove(0))
                        } else {
                            Ok(Value::Seq(values))
                        }
                    } else {
                        let ScVal::Map(Some(values)) = value else {
                            return Err(conversion("expected a struct map"));
                        };
                        values.validate().map_err(|e| conversion(e.to_string()))?;
                        if values.len() != s.fields.len() {
                            return Err(conversion("struct field count mismatch"));
                        }
                        let fields = s.fields.iter().map(|f| {
                            let name = f.name.to_utf8_string_lossy();
                            let field = values.iter().find(|e| {
                                matches!(&e.key, ScVal::Symbol(k) if k.as_slice() == name.as_bytes())
                            }).ok_or_else(|| conversion(format!("missing field {name}")))?;
                            Ok((Value::String(name.into()), decode(&field.val, &f.type_)?))
                        }).collect::<Result<_, SpecError>>()?;
                        Ok(Value::Map(fields))
                    }
                }
                ScSpecEntry::UdtUnionV0(u) => {
                    let values = vector(value, None)?;
                    let Some(ScVal::Symbol(name)) = values.first() else {
                        return Err(conversion("missing union tag"));
                    };
                    let case = u
                        .cases
                        .iter()
                        .find(|c| match c {
                            ScSpecUdtUnionCaseV0::VoidV0(c) => c.name.as_slice() == name.as_slice(),
                            ScSpecUdtUnionCaseV0::TupleV0(c) => {
                                c.name.as_slice() == name.as_slice()
                            }
                        })
                        .ok_or_else(|| conversion("unknown union variant"))?;
                    let name = Value::String(name.to_utf8_string_lossy().into());
                    match case {
                        ScSpecUdtUnionCaseV0::VoidV0(_) if values.len() == 1 => Ok(name),
                        ScSpecUdtUnionCaseV0::TupleV0(c) if values.len() == c.type_.len() + 1 => {
                            let mut payload = values[1..]
                                .iter()
                                .zip(c.type_.iter())
                                .map(|(v, t)| decode(v, t))
                                .collect::<Result<Vec<_>, _>>()?;
                            let payload = if payload.len() == 1 {
                                payload.remove(0)
                            } else {
                                Value::Seq(payload)
                            };
                            Ok(Value::Map(vec![(name, payload)]))
                        }
                        _ => Err(conversion("union payload arity mismatch")),
                    }
                }
                _ => Err(conversion("expected a container type")),
            },
            _ => Err(conversion("expected a container type")),
        }
    }

    fn encode_map_key(&self, value: Value, ty: &T, depth: usize) -> Result<ScVal, SpecError> {
        // Retain support for numeric/bool keys supplied through JSON objects,
        // whose property names arrive as strings rather than native numbers.
        if let Value::String(text) = &value {
            if matches!(
                ty,
                T::Bool | T::U32 | T::I32 | T::U64 | T::I64 | T::Timepoint | T::Duration
            ) {
                let json = serde_json::from_str(text).map_err(|e| conversion(e.to_string()))?;
                return self.encode_json(json, ty);
            }
            if let T::Udt(t) = ty {
                if matches!(
                    self.udt(&t.name.to_utf8_string_lossy())?,
                    ScSpecEntry::UdtEnumV0(_)
                ) {
                    if let Ok(number) = text.parse::<u32>() {
                        return self.encode_json(serde_json::Value::from(number), ty);
                    }
                }
            }
        }
        self.encode_native(value, ty, depth)
    }

    // JSON null is ambiguous to a general Serde buffer. Restore Option(None)
    // using the spec, including in struct/union leaves delegated to JSON.
    fn restore_options(&self, value: Value, ty: &T, depth: usize) -> Result<Value, SpecError> {
        nesting(depth)?;
        let restore = |v, t: &T| self.restore_options(v, t, depth + 1);
        match ty {
            T::Option(t) => Ok(Value::Option(if value == Value::Unit {
                None
            } else {
                Some(Box::new(restore(value, &t.value_type)?))
            })),
            T::Result(t) => restore(value, &t.ok_type),
            T::Vec(t) => {
                let Value::Seq(values) = value else {
                    return Err(conversion("expected a vector"));
                };
                Ok(Value::Seq(
                    values
                        .into_iter()
                        .map(|v| restore(v, &t.element_type))
                        .collect::<Result<_, _>>()?,
                ))
            }
            T::Tuple(t) => Ok(Value::Seq(
                sequence(value, t.value_types.len())?
                    .into_iter()
                    .zip(t.value_types.iter())
                    .map(|(v, t)| restore(v, t))
                    .collect::<Result<_, _>>()?,
            )),
            T::Udt(t) => match self.udt(&t.name.to_utf8_string_lossy())? {
                ScSpecEntry::UdtStructV0(s) => {
                    if s.fields.first().is_some_and(|f| f.name.as_slice() == b"0") {
                        if s.fields.len() == 1 {
                            return restore(value, &s.fields[0].type_);
                        }
                        Ok(Value::Seq(
                            sequence(value, s.fields.len())?
                                .into_iter()
                                .zip(s.fields.iter())
                                .map(|(v, f)| restore(v, &f.type_))
                                .collect::<Result<_, _>>()?,
                        ))
                    } else {
                        let Value::Map(values) = value else {
                            return Err(conversion("expected named struct fields"));
                        };
                        let mut fields = named_fields(values)?;
                        Ok(Value::Map(
                            s.fields
                                .iter()
                                .map(|f| {
                                    let name = f.name.to_utf8_string_lossy();
                                    let value = fields
                                        .remove(&name)
                                        .ok_or_else(|| conversion("missing struct field"))?;
                                    Ok((Value::String(name.into()), restore(value, &f.type_)?))
                                })
                                .collect::<Result<_, SpecError>>()?,
                        ))
                    }
                }
                ScSpecEntry::UdtUnionV0(u) => {
                    let (name, payload) = union(value)?;
                    if let Some(payload) = payload {
                        let c = u
                            .cases
                            .iter()
                            .find_map(|c| match c {
                                ScSpecUdtUnionCaseV0::TupleV0(c)
                                    if c.name.as_slice() == name.as_bytes() =>
                                {
                                    Some(c)
                                }
                                _ => None,
                            })
                            .ok_or_else(|| conversion("unknown union variant"))?;
                        let payload = if c.type_.len() == 1 {
                            restore(payload, &c.type_[0])?
                        } else {
                            Value::Seq(
                                sequence(payload, c.type_.len())?
                                    .into_iter()
                                    .zip(c.type_.iter())
                                    .map(|(v, t)| restore(v, t))
                                    .collect::<Result<_, _>>()?,
                            )
                        };
                        Ok(Value::Map(vec![(Value::String(name.into()), payload)]))
                    } else {
                        Ok(Value::String(name.into()))
                    }
                }
                _ => Ok(value),
            },
            _ => Ok(value),
        }
    }
}
