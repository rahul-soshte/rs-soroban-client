//! SEP-48 event layout validation and native parameter decoding.

use std::{borrow::Cow, collections::HashSet};

use serde::de::DeserializeOwned;
use stellar_baselib::{
    address::{Address, AddressTrait},
    xdr::{
        ContractEvent, ContractEventBody, ContractEventType, DiagnosticEvent, ScAddress,
        ScSpecEventDataFormat, ScSpecEventParamLocationV0, ScSpecEventV0, ScVal, Validate,
    },
};

use super::{native, ContractSpec, ScSpecEntry, SpecError};
use crate::{
    contract_client::Client,
    soroban_rpc::{EventResponse, EventType},
};

/// Failure to select or decode an event using its contract specification.
#[derive(Debug, thiserror::Error)]
pub enum EventDecodeError {
    /// Only contract events can be decoded with a contract interface.
    #[error("expected a contract event, not a system or diagnostic event")]
    NotContractEvent,
    /// An event from an unsuccessful contract call must not be treated as committed.
    #[error("event was emitted in an unsuccessful contract call")]
    UnsuccessfulCall,
    /// Invalid or missing emitter information.
    #[error("invalid event contract ID")]
    InvalidContractId,
    /// The event was emitted by a different contract than this client represents.
    #[error("event was emitted by {actual}, expected {expected}")]
    WrongContract {
        /// Expected contract ID.
        expected: String,
        /// Actual emitting contract ID.
        actual: String,
    },
    /// Malformed XDR in an RPC event response.
    #[error(transparent)]
    Rpc(#[from] crate::error::Error),
    /// No definition has this event name.
    #[error("event definition not found: {0}")]
    UnknownEvent(String),
    /// No declared event matches the static topic prefix.
    #[error("no event definition matches the topic prefix")]
    NoMatchingEvent,
    /// Matching prefixes exist, but none matches the complete payload.
    #[error("event payload does not match any of {events:?}; use decode_event_as for a specific definition")]
    NoMatchingShape {
        /// Definitions with matching topic prefixes.
        events: Vec<String>,
    },
    /// More than one complete interface definition matches the event.
    #[error("ambiguous event: matches {events:?}; use decode_event_as to select a definition")]
    Ambiguous {
        /// Names of all matching definitions.
        events: Vec<String>,
    },
    /// An invalid event definition was found in the contract interface.
    #[error("invalid event definition {event}: {reason}")]
    InvalidSpec {
        /// Event name.
        event: String,
        /// Invalid definition detail.
        reason: String,
    },
    /// Topics or data do not match the declared layout.
    #[error("invalid {event} event layout: {reason}")]
    InvalidLayout {
        /// Event name.
        event: String,
        /// Layout mismatch detail.
        reason: String,
    },
    /// A parameter does not match its declared contract type.
    #[error("invalid {event}.{parameter}: {source}")]
    InvalidValue {
        /// Event name.
        event: String,
        /// Parameter name.
        parameter: String,
        /// Conversion or interface lookup failure.
        #[source]
        source: Box<SpecError>,
    },
    /// The validated parameter set cannot be deserialized into the requested Rust type.
    #[error("cannot deserialize event parameters: {0}")]
    Deserialize(#[source] serde_content::Error),
}

/// Borrowed event input accepted by the typed event decoder.
///
/// Pass `&EventResponse`, `&ContractEvent`, or `&DiagnosticEvent` directly;
/// the conversion to this enum is automatic. Diagnostic wrappers are accepted
/// only for contract events in successful calls.
#[derive(Clone, Copy, Debug)]
pub enum EventRef<'a> {
    /// An event returned by `Server::get_events`.
    Rpc(&'a EventResponse),
    /// A raw XDR contract event, for example from a confirmed transaction.
    Contract(&'a ContractEvent),
    /// A contract event wrapped with simulation success information.
    Simulation(&'a DiagnosticEvent),
}

impl<'a> From<&'a EventResponse> for EventRef<'a> {
    fn from(value: &'a EventResponse) -> Self {
        Self::Rpc(value)
    }
}
impl<'a> From<&'a ContractEvent> for EventRef<'a> {
    fn from(value: &'a ContractEvent) -> Self {
        Self::Contract(value)
    }
}
impl<'a> From<&'a DiagnosticEvent> for EventRef<'a> {
    fn from(value: &'a DiagnosticEvent) -> Self {
        Self::Simulation(value)
    }
}

impl<'a> EventRef<'a> {
    fn contract_id(self) -> Result<String, EventDecodeError> {
        let address = match self {
            Self::Rpc(event) => {
                if event.event_type != EventType::Contract {
                    return Err(EventDecodeError::NotContractEvent);
                }
                if event.in_successful_contract_call == Some(false) {
                    return Err(EventDecodeError::UnsuccessfulCall);
                }
                Address::new(&event.contract_id)
                    .and_then(|a| a.to_sc_address())
                    .map_err(|_| EventDecodeError::InvalidContractId)?
            }
            Self::Simulation(event) => {
                if !event.in_successful_contract_call {
                    return Err(EventDecodeError::UnsuccessfulCall);
                }
                return Self::Contract(&event.event).contract_id();
            }
            Self::Contract(event) => {
                if event.type_ != ContractEventType::Contract {
                    return Err(EventDecodeError::NotContractEvent);
                }
                ScAddress::Contract(
                    event
                        .contract_id
                        .clone()
                        .ok_or(EventDecodeError::InvalidContractId)?,
                )
            }
        };
        if !matches!(address, ScAddress::Contract(_)) {
            return Err(EventDecodeError::InvalidContractId);
        }
        Ok(Address::from_sc_address(&address)
            .map_err(|_| EventDecodeError::InvalidContractId)?
            .to_string())
    }

    fn parts(self) -> Result<(Cow<'a, [ScVal]>, Cow<'a, ScVal>), EventDecodeError> {
        match self {
            Self::Rpc(event) => Ok((
                Cow::Owned(event.try_topic()?),
                Cow::Owned(event.try_value()?),
            )),
            Self::Contract(event) => {
                let ContractEventBody::V0(body) = &event.body;
                Ok((
                    Cow::Borrowed(body.topics.as_slice()),
                    Cow::Borrowed(&body.data),
                ))
            }
            Self::Simulation(event) => Self::Contract(&event.event).parts(),
        }
    }
}

fn prefix_matches(spec: &ScSpecEventV0, topics: &[ScVal]) -> bool {
    topics.len() >= spec.prefix_topics.len()
        && spec
            .prefix_topics
            .iter()
            .zip(topics)
            .all(|(prefix, topic)| match topic {
                ScVal::Symbol(s) => prefix.as_slice() == s.as_slice(),
                ScVal::String(s) => prefix.as_slice() == s.as_slice(),
                _ => false,
            })
}

fn deserialize<T: DeserializeOwned>(value: native::Value) -> Result<T, EventDecodeError> {
    serde_content::Deserializer::new(value)
        .human_readable()
        .coerce_numbers()
        .deserialize()
        .map_err(EventDecodeError::Deserialize)
}

impl ContractSpec {
    /// Find an event definition by its SEP-48 name.
    pub fn event(&self, name: &str) -> Result<&ScSpecEventV0, EventDecodeError> {
        self.entries()
            .iter()
            .find_map(|entry| match entry {
                ScSpecEntry::EventV0(event) if event.name.as_slice() == name.as_bytes() => {
                    Some(event)
                }
                _ => None,
            })
            .ok_or_else(|| EventDecodeError::UnknownEvent(name.into()))
    }

    /// Decode topic and data parameters into a native Rust struct. Selects a
    /// unique definition using its prefix and complete payload, independently
    /// of the requested Rust type. Does not bind the emitter to a contract ID;
    /// use `Client::decode_event` when that check is needed.
    pub fn decode_event<'a, T: DeserializeOwned>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<T, EventDecodeError> {
        let event = event.into();
        event.contract_id()?;
        let (topics, data) = event.parts()?;
        let candidates: Vec<_> = self
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ScSpecEntry::EventV0(spec) if prefix_matches(spec, &topics) => Some(spec),
                _ => None,
            })
            .collect();
        if candidates.is_empty() {
            return Err(EventDecodeError::NoMatchingEvent);
        }
        if candidates.len() == 1 {
            return deserialize(self.event_parameters(candidates[0], &topics, &data)?);
        }
        let names = candidates
            .iter()
            .map(|s| s.name.to_utf8_string_lossy())
            .collect();
        let mut matches = Vec::new();
        for spec in candidates {
            if self.event_values(spec, &topics, &data).is_ok() {
                matches.push(spec);
            }
        }
        match matches.len() {
            0 => Err(EventDecodeError::NoMatchingShape { events: names }),
            1 => deserialize(self.event_parameters(matches[0], &topics, &data)?),
            _ => Err(EventDecodeError::Ambiguous {
                events: matches
                    .into_iter()
                    .map(|spec| spec.name.to_utf8_string_lossy())
                    .collect(),
            }),
        }
    }

    /// Decode using a named event definition. The prefix, layout and parameter
    /// types are still validated; explicit selection does not bypass checks.
    pub fn decode_event_as<'a, T: DeserializeOwned>(
        &self,
        name: &str,
        event: impl Into<EventRef<'a>>,
    ) -> Result<T, EventDecodeError> {
        let event = event.into();
        event.contract_id()?;
        let (topics, data) = event.parts()?;
        deserialize(self.event_parameters(self.event(name)?, &topics, &data)?)
    }

    fn event_parameters(
        &self,
        spec: &ScSpecEventV0,
        topics: &[ScVal],
        data: &ScVal,
    ) -> Result<native::Value, EventDecodeError> {
        let values = self.event_values(spec, topics, data)?;
        let fields = spec
            .params
            .iter()
            .zip(values)
            .map(|(param, value)| {
                let parameter = param.name.to_utf8_string_lossy();
                let value = self.checked_native(value, &param.type_).map_err(|source| {
                    EventDecodeError::InvalidValue {
                        event: spec.name.to_utf8_string_lossy(),
                        parameter: parameter.clone(),
                        source: Box::new(source),
                    }
                })?;
                Ok((native::Value::String(parameter.into()), value))
            })
            .collect::<Result<_, EventDecodeError>>()?;
        Ok(native::Value::Map(fields))
    }

    // Match definitions using the wire interface alone. Native conversion
    // support and the caller's Rust type must not resolve schema ambiguity.
    fn event_values<'a>(
        &self,
        spec: &ScSpecEventV0,
        topics: &'a [ScVal],
        data: &'a ScVal,
    ) -> Result<Vec<&'a ScVal>, EventDecodeError> {
        use ScSpecEventParamLocationV0 as Location;
        let name = spec.name.to_utf8_string_lossy();
        let layout = |reason: &str| EventDecodeError::InvalidLayout {
            event: name.clone(),
            reason: reason.into(),
        };
        let invalid_spec = |reason: &str| EventDecodeError::InvalidSpec {
            event: name.clone(),
            reason: reason.into(),
        };
        let mut parameter_names = HashSet::new();
        for param in spec.params.iter() {
            std::str::from_utf8(param.name.as_slice())
                .map_err(|_| invalid_spec("parameter names must be UTF-8"))?;
            if !parameter_names.insert(param.name.as_slice()) {
                return Err(invalid_spec("duplicate parameter name"));
            }
        }
        let topic_count = spec
            .params
            .iter()
            .filter(|p| p.location == Location::TopicList)
            .count();
        let data_count = spec.params.len() - topic_count;
        if spec.data_format == ScSpecEventDataFormat::SingleValue && data_count > 1 {
            return Err(invalid_spec(
                "SINGLE_VALUE allows at most one data parameter",
            ));
        }
        if !prefix_matches(spec, topics) {
            return Err(layout("static topic prefix mismatch"));
        }
        if topics.len() != spec.prefix_topics.len() + topic_count {
            return Err(layout("topic count mismatch"));
        }
        let data_values: Vec<&ScVal> = match spec.data_format {
            ScSpecEventDataFormat::SingleValue if data_count == 0 => {
                if *data != ScVal::Void {
                    return Err(layout("expected void data for zero parameters"));
                }
                Vec::new()
            }
            ScSpecEventDataFormat::SingleValue => vec![data],
            ScSpecEventDataFormat::Vec => {
                let ScVal::Vec(Some(values)) = data else {
                    return Err(layout("expected vector data"));
                };
                if values.len() != data_count {
                    return Err(layout("data parameter count mismatch"));
                }
                values.iter().collect()
            }
            ScSpecEventDataFormat::Map => {
                let ScVal::Map(Some(values)) = data else {
                    return Err(layout("expected map data"));
                };
                values
                    .validate()
                    .map_err(|_| layout("invalid, duplicate or unsorted map entries"))?;
                if values.len() != data_count {
                    return Err(layout("data parameter count mismatch"));
                }
                let parameters = spec.params.iter().filter(|p| p.location == Location::Data);
                parameters
                    .map(|p| {
                        let entry = values.iter().find(|entry| {
                        matches!(&entry.key, ScVal::Symbol(s) if s.as_slice() == p.name.as_slice())
                    }).ok_or_else(|| layout("missing named data parameter"))?;
                        Ok(&entry.val)
                    })
                    .collect::<Result<_, EventDecodeError>>()?
            }
        };
        let mut topics = topics[spec.prefix_topics.len()..].iter();
        let mut data = data_values.into_iter();
        let mut values = Vec::with_capacity(spec.params.len());
        for param in spec.params.iter() {
            let value = match param.location {
                Location::TopicList => topics.next(),
                Location::Data => data.next(),
            }
            .ok_or_else(|| layout("missing parameter"))?;
            let parameter = param.name.to_utf8_string_lossy();
            self.check_wire_type(value, &param.type_, 0)
                .map_err(|source| EventDecodeError::InvalidValue {
                    event: name.clone(),
                    parameter: parameter.clone(),
                    source: Box::new(source),
                })?;
            values.push(value);
        }
        Ok(values)
    }
}

impl Client<'_> {
    /// Decode an RPC, confirmed XDR, or successful simulation contract event
    /// into a native struct. Rejects events emitted by another contract and
    /// definitions that remain ambiguous after complete payload validation.
    pub fn decode_event<'a, T: DeserializeOwned>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<T, EventDecodeError> {
        let event = event.into();
        self.check_event_contract(event)?;
        self.spec().decode_event(event)
    }

    /// Decode a named event from this contract, retaining all payload checks.
    pub fn decode_event_as<'a, T: DeserializeOwned>(
        &self,
        name: &str,
        event: impl Into<EventRef<'a>>,
    ) -> Result<T, EventDecodeError> {
        let event = event.into();
        self.check_event_contract(event)?;
        self.spec().decode_event_as(name, event)
    }

    fn check_event_contract(&self, event: EventRef<'_>) -> Result<(), EventDecodeError> {
        let actual = event.contract_id()?;
        if actual != self.contract_id() {
            return Err(EventDecodeError::WrongContract {
                expected: self.contract_id().into(),
                actual,
            });
        }
        Ok(())
    }
}
