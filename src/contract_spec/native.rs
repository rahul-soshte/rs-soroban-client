//! Preserve native map keys at the Serde boundary. Upstream JSON conversion is
//! retained for map-free subtrees; only map-containing containers need bridging.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use serde_content::{Data, Number};
use stellar_baselib::xdr::{ScMap, ScMapEntry, ScSpecTypeDef as T, Validate};

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
    fn contains_map(
        &self,
        ty: &T,
        seen: &mut HashSet<String>,
        depth: usize,
    ) -> Result<bool, SpecError> {
        nesting(depth)?;
        Ok(match ty {
            T::Map(_) => true,
            T::Option(t) => self.contains_map(&t.value_type, seen, depth + 1)?,
            T::Result(t) => self.contains_map(&t.ok_type, seen, depth + 1)?,
            T::Vec(t) => self.contains_map(&t.element_type, seen, depth + 1)?,
            T::Tuple(t) => {
                let mut found = false;
                for t in t.value_types.iter() {
                    found |= self.contains_map(t, seen, depth + 1)?;
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
                            found |= self.contains_map(&f.type_, seen, depth + 1)?;
                        }
                        found
                    }
                    ScSpecEntry::UdtUnionV0(u) => {
                        let mut found = false;
                        for c in u.cases.iter() {
                            if let ScSpecUdtUnionCaseV0::TupleV0(c) = c {
                                for t in c.type_.iter() {
                                    found |= self.contains_map(t, seen, depth + 1)?;
                                }
                            }
                        }
                        found
                    }
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
        if !self.contains_map(ty, &mut HashSet::new(), 0)? {
            return self.encode_json(to_json(value, depth)?, ty);
        }
        let encode = |v, t: &T| self.encode_native(v, t, depth + 1);
        match ty {
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
            T::Result(t) => encode(value, &t.ok_type),
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
            T::Tuple(t) => wire_vec(
                sequence(value, t.value_types.len())?
                    .into_iter()
                    .zip(t.value_types.iter())
                    .map(|(v, t)| encode(v, t))
                    .collect::<Result<_, _>>()?,
            ),
            T::Udt(t) => match self.udt(&t.name.to_utf8_string_lossy())? {
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
        if !self.contains_map(ty, &mut HashSet::new(), 0)? {
            return self.restore_options(from_json(self.decode_json(value, ty)?)?, ty, depth);
        }
        let decode = |v, t: &T| self.decode_native(v, t, depth + 1);
        match ty {
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
            T::Result(t) => decode(value, &t.ok_type),
            T::Vec(t) => Ok(Value::Seq(
                vector(value, None)?
                    .iter()
                    .map(|v| decode(v, &t.element_type))
                    .collect::<Result<_, _>>()?,
            )),
            T::Tuple(t) => Ok(Value::Seq(
                vector(value, Some(t.value_types.len()))?
                    .iter()
                    .zip(t.value_types.iter())
                    .map(|(v, t)| decode(v, t))
                    .collect::<Result<_, _>>()?,
            )),
            T::Udt(t) => match self.udt(&t.name.to_utf8_string_lossy())? {
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
    // using the spec, including when it occurs in a map-free struct/union leaf.
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
