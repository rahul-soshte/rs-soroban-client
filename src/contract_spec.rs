//! SEP-46 metadata, SEP-47 claims, and SEP-48 interfaces.
//!
//! Contract metadata and interfaces are untrusted self-descriptions, not proofs
//! of behavior. Native enums use Serde's externally tagged representation; their
//! numeric discriminants are obtained from the contract specification.

use std::io::Cursor;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use stellar_baselib::xdr::{
    Limited, Limits, ReadXdr, ScMetaEntry, ScSpecEntry, ScSpecFunctionV0, ScSpecTypeDef,
    ScSpecUdtUnionCaseV0, ScVal,
};

mod native;

/// Native 256-bit integer types, serialized losslessly as prefixed hex strings.
pub use ethnum::{I256, U256};

/// Failures while reading an interface or converting a native value.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// Invalid Wasm, XDR, or interface.
    #[error("invalid contract interface: {0}")]
    InvalidSpec(String),
    /// A function or user-defined type was not found.
    #[error("interface entry not found: {0}")]
    MissingEntry(String),
    /// A value does not match the declared interface.
    #[error("contract value conversion: {0}")]
    Conversion(String),
    /// Invalid SEP declaration; retains the original token.
    #[error("invalid SEP identifier: {0:?}")]
    InvalidSep(String),
    /// Network or ledger lookup failure.
    #[error(transparent)]
    Rpc(#[from] crate::error::Error),
}

fn conversion(message: impl Into<String>) -> SpecError {
    SpecError::Conversion(message.into())
}

/// One metadata entry. Duplicates and order are significant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataEntry {
    /// Metadata key.
    pub key: String,
    /// Metadata value.
    pub value: String,
}

/// Ordered metadata from every `contractmetav0` section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContractMetadata {
    /// Entries in Wasm order, including duplicate keys.
    pub entries: Vec<MetadataEntry>,
}

impl ContractMetadata {
    /// Read SEP-46 entries, decoding each custom section independently.
    pub fn from_wasm(wasm: &[u8]) -> Result<Self, SpecError> {
        let mut entries = Vec::new();
        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            let payload = payload.map_err(|e| SpecError::InvalidSpec(e.to_string()))?;
            if let wasmparser::Payload::CustomSection(section) = payload {
                if section.name() == "contractmetav0" {
                    let mut reader = Limited::new(
                        Cursor::new(section.data()),
                        Limits {
                            depth: 100,
                            len: 0x1000000,
                        },
                    );
                    for entry in ScMetaEntry::read_xdr_iter(&mut reader) {
                        let ScMetaEntry::ScMetaV0(meta) =
                            entry.map_err(|e| SpecError::InvalidSpec(e.to_string()))?;
                        let key = String::from_utf8(meta.key.into_vec())
                            .map_err(|e| SpecError::InvalidSpec(e.to_string()))?;
                        let value = String::from_utf8(meta.val.into_vec())
                            .map_err(|e| SpecError::InvalidSpec(e.to_string()))?;
                        entries.push(MetadataEntry { key, value });
                    }
                }
            }
        }
        Ok(Self { entries })
    }

    /// Declared SEP numbers, deduplicated in declaration order.
    ///
    /// These are unverified claims. Whitespace, empty tokens, and identifiers
    /// outside `u32` are rejected rather than silently ignored. Leading zeroes
    /// are tolerated (SEP-47 recommends, but does not require, omitting them).
    pub fn sep_claims(&self) -> Result<Vec<u32>, SpecError> {
        let mut claims = Vec::new();
        for entry in self.entries.iter().filter(|e| e.key == "sep") {
            for token in entry.value.split(',') {
                if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(SpecError::InvalidSep(token.into()));
                }
                let number = token
                    .parse()
                    .map_err(|_| SpecError::InvalidSep(token.into()))?;
                if !claims.contains(&number) {
                    claims.push(number);
                }
            }
        }
        Ok(claims)
    }
}

/// Complete SEP-48 interface, including function/type/event documentation.
#[derive(Clone)]
pub struct ContractSpec {
    entries: Vec<ScSpecEntry>,
    tools: soroban_spec_tools::Spec,
}

impl ContractSpec {
    /// Parse the interface embedded in a contract's Wasm.
    pub fn from_wasm(wasm: &[u8]) -> Result<Self, SpecError> {
        let mut sections = 0;
        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            if let wasmparser::Payload::CustomSection(section) =
                payload.map_err(|e| SpecError::InvalidSpec(e.to_string()))?
            {
                if section.name() == "contractspecv0" {
                    sections += 1;
                }
            }
        }
        if sections > 1 {
            return Err(SpecError::InvalidSpec(
                "SEP-48 requires a single contractspecv0 section".into(),
            ));
        }
        let entries = soroban_spec::read::from_wasm(wasm)
            .map_err(|e| SpecError::InvalidSpec(e.to_string()))?;
        Self::new(entries)
    }

    /// Construct an interface from decoded entries (also useful offline).
    pub fn new(entries: Vec<ScSpecEntry>) -> Result<Self, SpecError> {
        let mut names = std::collections::HashSet::new();
        for entry in &entries {
            // Function, type and event namespaces need not be disjoint.
            let key = match entry {
                ScSpecEntry::FunctionV0(x) => (0, x.name.to_utf8_string_lossy()),
                ScSpecEntry::EventV0(x) => (1, x.name.to_utf8_string_lossy()),
                ScSpecEntry::UdtStructV0(x) => (2, x.name.to_utf8_string_lossy()),
                ScSpecEntry::UdtUnionV0(x) => (2, x.name.to_utf8_string_lossy()),
                ScSpecEntry::UdtEnumV0(x) => (2, x.name.to_utf8_string_lossy()),
                ScSpecEntry::UdtErrorEnumV0(x) => (2, x.name.to_utf8_string_lossy()),
            };
            if !names.insert(key) {
                return Err(SpecError::InvalidSpec("duplicate interface entry".into()));
            }
        }
        // Conversion only needs types. Avoid upstream's shared-name lookup
        // confusing a function/event with a type in a separate namespace.
        let mut wire_entries: Vec<_> = entries
            .iter()
            .filter(|e| !matches!(e, ScSpecEntry::FunctionV0(_) | ScSpecEntry::EventV0(_)))
            .cloned()
            .collect();
        // Upstream decodes named structs by zipping fields with the sorted wire
        // map. Sort only its private copy; preserve the public declaration order.
        for entry in &mut wire_entries {
            if let ScSpecEntry::UdtStructV0(s) = entry {
                if !s.fields.iter().any(|f| f.name.as_slice() == b"0") {
                    let mut fields = s.fields.to_vec();
                    fields.sort_by(|a, b| a.name.cmp(&b.name));
                    s.fields = fields
                        .try_into()
                        .map_err(|e: stellar_baselib::xdr::Error| {
                            SpecError::InvalidSpec(e.to_string())
                        })?;
                }
            }
        }
        Ok(Self {
            entries,
            tools: soroban_spec_tools::Spec::new(&wire_entries),
        })
    }

    /// All original entries, without discarding event or documentation entries.
    pub fn entries(&self) -> &[ScSpecEntry] {
        &self.entries
    }

    /// Find a callable function by name.
    pub fn function(&self, name: &str) -> Result<&ScSpecFunctionV0, SpecError> {
        self.entries
            .iter()
            .find_map(|e| match e {
                ScSpecEntry::FunctionV0(f) if f.name.as_slice() == name.as_bytes() => Some(f),
                _ => None,
            })
            .ok_or_else(|| SpecError::MissingEntry(name.into()))
    }

    fn udt(&self, name: &str) -> Result<&ScSpecEntry, SpecError> {
        self.entries
            .iter()
            .find(|e| match e {
                ScSpecEntry::UdtStructV0(s) => s.name.as_slice() == name.as_bytes(),
                ScSpecEntry::UdtUnionV0(s) => s.name.as_slice() == name.as_bytes(),
                ScSpecEntry::UdtEnumV0(s) => s.name.as_slice() == name.as_bytes(),
                ScSpecEntry::UdtErrorEnumV0(s) => s.name.as_slice() == name.as_bytes(),
                _ => false,
            })
            .ok_or_else(|| SpecError::MissingEntry(name.into()))
    }

    /// Convert positional tuples or named argument structs into invocation values.
    /// Use `()` for a function with no arguments; `(value,)` for one argument.
    pub fn encode_args<A: Serialize>(
        &self,
        function: &str,
        args: &A,
    ) -> Result<Vec<ScVal>, SpecError> {
        let f = self.function(function)?;
        let value = native::serialize(args)?;
        let values = match value {
            native::Value::Unit if f.inputs.is_empty() => Vec::new(),
            native::Value::Seq(values) if values.len() == f.inputs.len() => values,
            native::Value::Map(values) if values.len() == f.inputs.len() => {
                let mut values = native::named_fields(values)?;
                f.inputs
                    .iter()
                    .map(|input| {
                        values
                            .remove(&input.name.to_utf8_string_lossy())
                            .ok_or_else(|| conversion(format!("missing argument {}", input.name)))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            _ => {
                return Err(conversion(
                    "arguments must match the function's named inputs or positional arity",
                ))
            }
        };
        values
            .into_iter()
            .zip(f.inputs.iter())
            .map(|(v, input)| self.encode_native(v, &input.type_, 0))
            .collect()
    }

    /// Convert a Rust value according to an interface type.
    /// Native maps support composite keys, including tuples and structs, and
    /// are sorted by encoded key order regardless of their Rust iteration order.
    pub fn encode<T: Serialize>(&self, value: &T, ty: &ScSpecTypeDef) -> Result<ScVal, SpecError> {
        self.encode_native(native::serialize(value)?, ty, 0)
    }

    fn encode_json(&self, value: Value, ty: &ScSpecTypeDef) -> Result<ScVal, SpecError> {
        if let ScSpecTypeDef::Result(inner) = ty {
            return self.encode_json(value, &inner.ok_type);
        }
        let value = self.normalize(value, ty, false, 0)?;
        upstream(|| self.tools.from_json(&value, ty))
    }

    /// Decode a value into an ordinary Rust value.
    pub fn decode<T: DeserializeOwned>(
        &self,
        value: &ScVal,
        ty: &ScSpecTypeDef,
    ) -> Result<T, SpecError> {
        let native = self.decode_native(value, ty, 0)?;
        if self.encode_native(native::simplify(native.clone(), 0)?, ty, 0)? != *value {
            return Err(conversion("result does not match the declared wire type"));
        }
        serde_content::Deserializer::new(native)
            .human_readable()
            .coerce_numbers()
            .deserialize()
            .map_err(|e| conversion(e.to_string()))
    }

    fn decode_json(&self, value: &ScVal, ty: &ScSpecTypeDef) -> Result<Value, SpecError> {
        let json = upstream(|| self.tools.xdr_to_json(value, ty))?;
        // Reject truncated containers, missing fields, invalid discriminants,
        // wrong wire tags, and other lossy upstream conversions.
        let native = self.normalize(json, ty, true, 0)?;
        if self.encode_json(native.clone(), ty)? != *value {
            return Err(conversion("result does not match the declared wire type"));
        }
        Ok(native)
    }

    /// Decode the successful output of a named function. Contract errors must
    /// be handled separately before calling this method.
    pub fn decode_result<T: DeserializeOwned>(
        &self,
        function: &str,
        value: &ScVal,
    ) -> Result<T, SpecError> {
        let f = self.function(function)?;
        let ty = f.outputs.first().unwrap_or(&ScSpecTypeDef::Void);
        let ty = match ty {
            ScSpecTypeDef::Result(r) => &r.ok_type,
            other => other,
        };
        self.decode(value, ty)
    }

    // Adapt map-free Serde shapes to upstream JSON shapes. Native maps are
    // handled separately so their keys never need to become JSON strings.
    fn normalize(
        &self,
        value: Value,
        ty: &ScSpecTypeDef,
        decode: bool,
        depth: usize,
    ) -> Result<Value, SpecError> {
        use ScSpecTypeDef as T;
        if depth > 100 {
            return Err(conversion("value nesting exceeds 100"));
        }
        let recurse = |v, t: &T| self.normalize(v, t, decode, depth + 1);
        match ty {
            T::I128 | T::U128 | T::I256 | T::U256 => {
                let text = match value {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s,
                    _ => return Err(conversion("expected an integer")),
                };
                if decode && matches!(ty, T::I128 | T::U128) {
                    serde_json::from_str(&text).map_err(|e| conversion(e.to_string()))
                } else if decode && matches!(ty, T::I256) {
                    Ok(Value::String(format!(
                        "{:-#x}",
                        I256::from_str_prefixed(&text).map_err(|e| conversion(e.to_string()))?
                    )))
                } else if decode && matches!(ty, T::U256) {
                    Ok(Value::String(format!(
                        "{:#x}",
                        U256::from_str_prefixed(&text).map_err(|e| conversion(e.to_string()))?
                    )))
                } else {
                    Ok(Value::String(text))
                }
            }
            T::Void => {
                if decode && value.is_null() {
                    Ok(value)
                } else if !decode && value.is_null() {
                    Ok(Value::String("void".into()))
                } else {
                    Err(conversion("expected unit"))
                }
            }
            T::Timepoint | T::Duration => {
                let number = value
                    .as_u64()
                    .ok_or_else(|| conversion("expected seconds as u64"))?;
                if decode {
                    Ok(Value::from(number))
                } else if matches!(ty, T::Timepoint) {
                    Ok(serde_json::json!({"timepoint": number}))
                } else {
                    Ok(serde_json::json!({"duration": number}))
                }
            }
            T::Address | T::MuxedAddress => {
                let address = upstream(|| soroban_spec_tools::from_json_primitives(&value, ty))?;
                if matches!(ty, T::Address)
                    && !matches!(
                        address,
                        ScVal::Address(
                            stellar_baselib::xdr::ScAddress::Account(_)
                                | stellar_baselib::xdr::ScAddress::Contract(_)
                        )
                    )
                {
                    return Err(conversion(
                        "muxed address is not allowed by the address type",
                    ));
                }
                Ok(value)
            }
            T::Option(inner) => {
                if value.is_null() {
                    Ok(value)
                } else {
                    recurse(value, &inner.value_type)
                }
            }
            T::Result(inner) => recurse(value, &inner.ok_type),
            T::Vec(inner) => {
                let Value::Array(values) = value else {
                    return Err(conversion("expected a vector"));
                };
                Ok(Value::Array(
                    values
                        .into_iter()
                        .map(|v| recurse(v, &inner.element_type))
                        .collect::<Result<_, _>>()?,
                ))
            }
            T::Tuple(inner) => {
                let Value::Array(values) = value else {
                    return Err(conversion("expected a tuple"));
                };
                if values.len() != inner.value_types.len() {
                    return Err(conversion("tuple arity mismatch"));
                }
                Ok(Value::Array(
                    values
                        .into_iter()
                        .zip(inner.value_types.iter())
                        .map(|(v, t)| recurse(v, t))
                        .collect::<Result<_, _>>()?,
                ))
            }
            T::Bytes | T::BytesN(_) => {
                let bytes = match &value {
                    Value::String(s) => hex::decode(s).map_err(|e| conversion(e.to_string()))?,
                    Value::Array(items) => items
                        .iter()
                        .map(|v| {
                            v.as_u64()
                                .and_then(|n| u8::try_from(n).ok())
                                .ok_or_else(|| conversion("invalid byte"))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(conversion("expected bytes")),
                };
                if let T::BytesN(n) = ty {
                    if bytes.len() != n.n as usize {
                        return Err(conversion("fixed byte length mismatch"));
                    }
                }
                if decode {
                    Ok(serde_json::json!(bytes))
                } else {
                    Ok(Value::String(hex::encode(bytes)))
                }
            }
            T::Udt(reference) => match self.udt(&reference.name.to_utf8_string_lossy())? {
                ScSpecEntry::UdtStructV0(s) => {
                    if s.fields.first().is_some_and(|f| f.name.as_slice() == b"0") {
                        // Serde represents a single-field tuple struct as a
                        // newtype (a scalar), while SEP-48 uses a one-item vec.
                        if s.fields.len() == 1 {
                            if decode {
                                let Value::Array(mut values) = value else {
                                    return Err(conversion("expected a newtype wire vector"));
                                };
                                if values.len() != 1 {
                                    return Err(conversion("newtype arity mismatch"));
                                }
                                return recurse(values.remove(0), &s.fields[0].type_);
                            }
                            return Ok(Value::Array(vec![recurse(value, &s.fields[0].type_)?]));
                        }
                        let Value::Array(values) = value else {
                            return Err(conversion("expected a tuple struct"));
                        };
                        if values.len() != s.fields.len() {
                            return Err(conversion("tuple struct arity mismatch"));
                        }
                        Ok(Value::Array(
                            values
                                .into_iter()
                                .zip(s.fields.iter())
                                .map(|(v, f)| recurse(v, &f.type_))
                                .collect::<Result<_, _>>()?,
                        ))
                    } else {
                        let Value::Object(mut values) = value else {
                            return Err(conversion("expected a named struct"));
                        };
                        if values.len() != s.fields.len() {
                            return Err(conversion("struct field count mismatch"));
                        }
                        Ok(Value::Object(
                            s.fields
                                .iter()
                                .map(|f| {
                                    let name = f.name.to_utf8_string_lossy();
                                    let v = values.remove(&name).ok_or_else(|| {
                                        conversion(format!("missing field {name}"))
                                    })?;
                                    Ok((name, recurse(v, &f.type_)?))
                                })
                                .collect::<Result<_, SpecError>>()?,
                        ))
                    }
                }
                ScSpecEntry::UdtEnumV0(e) => {
                    let case = e
                        .cases
                        .iter()
                        .find(|c| match &value {
                            Value::String(name) => c.name.as_slice() == name.as_bytes(),
                            Value::Number(n) => n.as_u64() == Some(c.value.into()),
                            _ => false,
                        })
                        .ok_or_else(|| conversion("unknown enum variant"))?;
                    if decode {
                        Ok(Value::String(case.name.to_utf8_string_lossy()))
                    } else {
                        Ok(Value::from(case.value))
                    }
                }
                ScSpecEntry::UdtUnionV0(u) => {
                    let (name, payload) = match value {
                        Value::String(name) => (name, None),
                        Value::Object(values) if values.len() == 1 => {
                            let (name, payload) = values.into_iter().next().unwrap();
                            (name, Some(payload))
                        }
                        _ => return Err(conversion("expected an externally tagged union")),
                    };
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
                    match (case, payload) {
                        (ScSpecUdtUnionCaseV0::VoidV0(_), None) => Ok(Value::String(name)),
                        (ScSpecUdtUnionCaseV0::TupleV0(c), Some(payload)) => {
                            let payload = if c.type_.len() == 1 {
                                recurse(payload, &c.type_[0])?
                            } else {
                                let Value::Array(values) = payload else {
                                    return Err(conversion("expected a tuple payload"));
                                };
                                if values.len() != c.type_.len() {
                                    return Err(conversion("union payload arity mismatch"));
                                }
                                Value::Array(
                                    values
                                        .into_iter()
                                        .zip(c.type_.iter())
                                        .map(|(v, t)| recurse(v, t))
                                        .collect::<Result<_, _>>()?,
                                )
                            };
                            Ok(Value::Object([(name, payload)].into_iter().collect()))
                        }
                        _ => Err(conversion("union payload mismatch")),
                    }
                }
                _ => Err(conversion(
                    "error enums are decoded through the contract error boundary",
                )),
            },
            _ => Ok(value),
        }
    }
}

// Upstream documents panics for unsupported or mismatched values. Contain that
// boundary so malformed, untrusted interfaces never unwind through callers.
fn upstream<T>(f: impl FnOnce() -> Result<T, soroban_spec_tools::Error>) -> Result<T, SpecError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|_| conversion("unsupported or malformed value in upstream spec conversion"))?
        .map_err(|e| conversion(e.to_string()))
}

impl crate::Server {
    /// Fetch and parse a deployed contract's SEP-48 interface.
    pub async fn get_contract_spec(&self, contract_id: &str) -> Result<ContractSpec, SpecError> {
        ContractSpec::from_wasm(&self.get_contract_wasm_by_contract_id(contract_id).await?)
    }

    /// Fetch all SEP-46 metadata entries from a deployed contract.
    pub async fn get_contract_metadata(
        &self,
        contract_id: &str,
    ) -> Result<ContractMetadata, SpecError> {
        ContractMetadata::from_wasm(&self.get_contract_wasm_by_contract_id(contract_id).await?)
    }

    /// Fetch unverified SEP-47 declarations from a deployed contract.
    pub async fn get_contract_sep_claims(&self, contract_id: &str) -> Result<Vec<u32>, SpecError> {
        self.get_contract_metadata(contract_id).await?.sep_claims()
    }
}
