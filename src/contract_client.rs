//! A runtime-discovered contract client. Calls name functions and accept native
//! tuples or named argument structs; no caller-side `ScVal` construction is needed.
//!
//! Simulation does not submit a transaction. Preparation returns an unsigned
//! transaction and a *predicted* return value; decode the confirmed transaction
//! separately after signing, sending and waiting with the existing server APIs.

use serde::{de::DeserializeOwned, Serialize};
use stellar_baselib::{
    account::Account,
    address::{Address, AddressTrait},
    operation::Operation,
    transaction::Transaction,
    transaction_builder::{TransactionBuilder, TransactionBuilderBehavior},
    xdr::{
        ContractEventBody, ContractEventType, DiagnosticEvent, ScAddress, ScError, ScSpecEntry,
        ScSpecTypeDef, ScVal,
    },
};

use crate::{
    contract_spec::{ContractMetadata, ContractSpec, SpecError},
    soroban_rpc::{SimulateTransactionResponse, TransactionDetails, TransactionStatus},
    transaction::assemble_transaction,
    Server, SimulationOptions,
};

/// One possible symbolic meaning for a contract error code.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorCase {
    /// Name of the error enum in the interface.
    pub enum_name: String,
    /// Name of the variant.
    pub variant: String,
    /// Variant documentation.
    pub doc: String,
}

/// A contract failure, preserving its numeric code even if no enum matches.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
#[error("contract error {code} ({candidates:?})")]
pub struct ContractFailure {
    /// Contract error code.
    pub code: u32,
    /// Possible symbolic meanings. More than one means the spec is ambiguous.
    pub candidates: Vec<ErrorCase>,
}

/// Failure of a typed invocation. `E` is the caller's Serde-deserializable enum.
#[derive(Debug, thiserror::Error)]
pub enum CallError<E = ContractFailure> {
    /// A contract-specific Rust error variant.
    #[error("contract returned a typed error")]
    Contract(E),
    /// Unknown, ambiguous, or unmappable contract error; the code is preserved.
    #[error(transparent)]
    UnknownContract(ContractFailure),
    /// Interface lookup or native conversion failure.
    #[error(transparent)]
    Spec(#[from] SpecError),
    /// RPC, transaction assembly, or restoration failure.
    #[error(transparent)]
    Rpc(#[from] crate::error::Error),
    /// Simulation failed without a reliably identifiable top-level contract error.
    #[error("simulation failed: {message}")]
    Simulation {
        /// Original RPC message, retained for diagnostics, never parsed for codes.
        message: String,
        /// Structured diagnostic events.
        diagnostics: Vec<DiagnosticEvent>,
    },
    /// The response is missing a required value or is not a confirmed success.
    #[error("invalid invocation response: {0}")]
    InvalidResponse(String),
}

/// Fee, expiration, and simulation settings for an invocation.
pub struct InvocationOptions {
    /// Initial inclusion fee in stroops, before simulated resource fees.
    pub fee: u32,
    /// Transaction timeout in seconds. Must be positive.
    pub timeout_seconds: i64,
    /// Optional simulation/authentication settings.
    pub simulation: Option<SimulationOptions>,
}

impl Default for InvocationOptions {
    fn default() -> Self {
        Self {
            fee: 1000,
            timeout_seconds: 60,
            simulation: None,
        }
    }
}

/// An unsigned prepared transaction and its predicted result.
pub struct PreparedInvocation<T> {
    /// Transaction assembled using the simulation's footprint, fees and auth.
    pub transaction: Transaction,
    /// Predicted value; not a confirmed on-chain result.
    pub simulated_value: T,
}

/// A contract interface snapshot discovered over RPC.
///
/// Recreate the client after upgrading a contract. No global cache is used;
/// metadata and interface are parsed from the same fetched Wasm.
pub struct Client<'a> {
    server: &'a Server,
    contract_id: String,
    address: ScAddress,
    spec: ContractSpec,
    metadata: ContractMetadata,
}

impl<'a> Client<'a> {
    /// Fetch a contract's Wasm and discover its interface and metadata.
    pub async fn new(server: &'a Server, contract_id: &str) -> Result<Self, SpecError> {
        let address = Address::new(contract_id)
            .and_then(|a| a.to_sc_address())
            .map_err(|e| SpecError::InvalidSpec(e.to_string()))?;
        if !matches!(address, ScAddress::Contract(_)) {
            return Err(SpecError::InvalidSpec("expected a contract address".into()));
        }
        let wasm = server.get_contract_wasm_by_contract_id(contract_id).await?;
        Ok(Self {
            server,
            contract_id: contract_id.into(),
            address,
            spec: ContractSpec::from_wasm(&wasm)?,
            metadata: ContractMetadata::from_wasm(&wasm)?,
        })
    }

    /// Contract ID used for all invocations.
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    /// The discovered interface, including event specifications.
    pub fn spec(&self) -> &ContractSpec {
        &self.spec
    }

    /// Ordered metadata from the same Wasm as the interface.
    pub fn metadata(&self) -> &ContractMetadata {
        &self.metadata
    }

    /// Unverified SEP-47 claims.
    pub fn sep_claims(&self) -> Result<Vec<u32>, SpecError> {
        self.metadata.sep_claims()
    }

    /// Validate and convert arguments, producing a reusable invocation.
    pub fn call<A: Serialize>(
        &self,
        function: &str,
        args: &A,
    ) -> Result<Invocation<'_, 'a>, SpecError> {
        let args = self.spec.encode_args(function, args)?;
        let operation = Operation::new()
            .invoke_contract(&self.contract_id, function, args, None)
            .map_err(|e| SpecError::Conversion(format!("{e:?}")))?;
        Ok(Invocation {
            client: self,
            function: function.into(),
            operation,
        })
    }
}

/// A validated function name and encoded native arguments.
pub struct Invocation<'c, 's> {
    client: &'c Client<'s>,
    function: String,
    operation: stellar_baselib::xdr::Operation,
}

impl Invocation<'_, '_> {
    /// The existing low-level operation, for custom transaction/auth workflows.
    pub fn operation(&self) -> stellar_baselib::xdr::Operation {
        self.operation.clone()
    }

    /// Build an unsigned transaction. The source account is a snapshot and is
    /// not advanced; callers must manage sequence numbers for concurrent sends.
    pub fn transaction(
        &self,
        source: &Account,
        network: &str,
        options: &InvocationOptions,
    ) -> Result<Transaction, SpecError> {
        if options.timeout_seconds <= 0 {
            return Err(SpecError::Conversion("timeout must be positive".into()));
        }
        let mut source = source.clone();
        let mut builder = TransactionBuilder::new(&mut source, network, None);
        builder.fee(options.fee).add_operation(self.operation());
        builder
            .set_timeout(options.timeout_seconds)
            .map_err(SpecError::Conversion)?;
        Ok(builder.build())
    }

    /// Simulate a call and return a native value, without submitting it.
    pub async fn simulate<T: DeserializeOwned>(
        &self,
        source: &Account,
        network: &str,
    ) -> Result<T, CallError> {
        self.try_simulate(source, network).await
    }

    /// Simulate with a caller-defined contract error enum. Derive
    /// `serde::Deserialize` on `E`; variant names must match the specification
    /// (or use Serde rename attributes).
    pub async fn try_simulate<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        source: &Account,
        network: &str,
    ) -> Result<T, CallError<E>> {
        self.try_simulate_with_options(source, network, InvocationOptions::default())
            .await
    }

    /// Simulate with explicit fee, timeout, and authentication settings.
    pub async fn try_simulate_with_options<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        source: &Account,
        network: &str,
        options: InvocationOptions,
    ) -> Result<T, CallError<E>> {
        let tx = self.transaction(source, network, &options)?;
        let sim = self
            .client
            .server
            .simulate_transaction(&tx, options.simulation)
            .await?;
        self.simulated_value(&sim)
    }

    /// Prepare an unsigned transaction and return its predicted native value.
    pub async fn prepare<T: DeserializeOwned>(
        &self,
        source: &Account,
        network: &str,
    ) -> Result<PreparedInvocation<T>, CallError> {
        self.try_prepare(source, network, InvocationOptions::default())
            .await
    }

    /// Prepare with custom options and caller-defined error variants. Simulation
    /// is performed exactly once and reused by the existing assembly path.
    pub async fn try_prepare<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        source: &Account,
        network: &str,
        options: InvocationOptions,
    ) -> Result<PreparedInvocation<T>, CallError<E>> {
        let tx = self.transaction(source, network, &options)?;
        let sim = self
            .client
            .server
            .simulate_transaction(&tx, options.simulation)
            .await?;
        self.check_simulation::<E>(&sim)?;
        // Restoration is a distinct actionable error, not a missing return value.
        if let Some((fee, data)) = sim.try_restore_transaction_data()? {
            return Err(crate::error::Error::RestorationRequired(fee, data).into());
        }
        let value = self.simulated_value(&sim)?;
        // Guard the older assembly helper's required-field assumptions.
        if sim
            .min_resource_fee
            .as_ref()
            .and_then(|s| s.parse::<u32>().ok())
            .and_then(|fee| tx.fee.checked_add(fee))
            .is_none()
            || sim.try_transaction_data()?.is_none()
        {
            return Err(CallError::InvalidResponse(
                "missing or invalid assembly data".into(),
            ));
        }
        Ok(PreparedInvocation {
            transaction: assemble_transaction(&tx, sim)?,
            simulated_value: value,
        })
    }

    /// Decode the actual confirmed transaction result, not its simulation.
    pub fn decode_transaction<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        result: &TransactionDetails,
    ) -> Result<T, CallError<E>> {
        match result.status {
            TransactionStatus::Success => {
                let value = result
                    .to_result_meta()
                    .and_then(|(_, v)| v)
                    .ok_or_else(|| {
                        CallError::InvalidResponse("missing confirmed return value".into())
                    })?;
                self.decode_value(&value)
            }
            TransactionStatus::Failed => {
                let events = result.to_diagnostic_events().unwrap_or_default();
                if let Some(code) = root_contract_error(&events, &self.client.address) {
                    return Err(self.contract_error(code));
                }
                Err(CallError::InvalidResponse(
                    "transaction failed; no structured contract error available".into(),
                ))
            }
            _ => Err(CallError::InvalidResponse(
                "transaction is not confirmed".into(),
            )),
        }
    }

    fn simulated_value<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        sim: &SimulateTransactionResponse,
    ) -> Result<T, CallError<E>> {
        self.check_simulation(sim)?;
        if let Some((fee, data)) = sim.try_restore_transaction_data()? {
            return Err(crate::error::Error::RestorationRequired(fee, data).into());
        }
        let (value, _) = sim
            .try_result()?
            .ok_or_else(|| CallError::InvalidResponse("missing simulation result".into()))?;
        self.decode_value(&value)
    }

    fn decode_value<T: DeserializeOwned, E: DeserializeOwned>(
        &self,
        value: &ScVal,
    ) -> Result<T, CallError<E>> {
        if let ScVal::Error(ScError::Contract(code)) = value {
            return Err(self.contract_error(*code));
        }
        Ok(self.client.spec.decode_result(&self.function, value)?)
    }

    fn check_simulation<E: DeserializeOwned>(
        &self,
        sim: &SimulateTransactionResponse,
    ) -> Result<(), CallError<E>> {
        if let Some(message) = &sim.error {
            let diagnostics = sim.to_events().unwrap_or_default();
            if let Some(code) = root_contract_error(&diagnostics, &self.client.address) {
                return Err(self.contract_error(code));
            }
            return Err(CallError::Simulation {
                message: message.clone(),
                diagnostics,
            });
        }
        Ok(())
    }

    fn contract_error<E: DeserializeOwned>(&self, code: u32) -> CallError<E> {
        let failure = self.client.spec.contract_error(&self.function, code);
        if failure.candidates.len() == 1 {
            if let Ok(variant) = serde_json::from_value(serde_json::Value::String(
                failure.candidates[0].variant.clone(),
            )) {
                return CallError::Contract(variant);
            }
        }
        // The default structured error type can be deserialized from its full
        // representation without inventing a symbolic variant.
        if let Ok(value) = serde_json::from_value(
            serde_json::to_value(&failure).expect("serializable error details"),
        ) {
            return CallError::Contract(value);
        }
        CallError::UnknownContract(failure)
    }
}

impl ContractSpec {
    /// Resolve a contract code without assuming its enum is named `Error`.
    /// A function's explicit error UDT takes precedence over other enums.
    pub fn contract_error(&self, function: &str, code: u32) -> ContractFailure {
        let error_type = self
            .function(function)
            .ok()
            .and_then(|f| f.outputs.first())
            .and_then(|t| match t {
                ScSpecTypeDef::Result(r) => match r.error_type.as_ref() {
                    ScSpecTypeDef::Udt(u) => Some(u.name.as_slice()),
                    _ => None,
                },
                _ => None,
            });
        let candidates = self
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ScSpecEntry::UdtErrorEnumV0(e)
                    if error_type.is_none_or(|name| e.name.as_slice() == name) =>
                {
                    Some(e)
                }
                _ => None,
            })
            .flat_map(|e| {
                e.cases
                    .iter()
                    .filter(move |c| c.value == code)
                    .map(move |c| ErrorCase {
                        enum_name: e.name.to_utf8_string_lossy(),
                        variant: c.name.to_utf8_string_lossy(),
                        doc: c.doc.to_utf8_string_lossy(),
                    })
            })
            .collect();
        ContractFailure { code, candidates }
    }
}

// Only inspect the final diagnostic error, and only attribute a contract code
// to this invocation if it belongs to the root contract. Earlier errors may
// have been caught by the contract, including failures of nested calls.
fn root_contract_error(events: &[DiagnosticEvent], address: &ScAddress) -> Option<u32> {
    let ScAddress::Contract(root) = address else {
        return None;
    };
    let event = events.iter().rev().find(|d| {
        let ContractEventBody::V0(body) = &d.event.body;
        !d.in_successful_contract_call
            && d.event.type_ == ContractEventType::Diagnostic
            && matches!(body.topics.first(), Some(ScVal::Symbol(s)) if s.as_slice() == b"error")
    })?;
    if event.event.contract_id.as_ref() != Some(root) {
        return None;
    }
    let ContractEventBody::V0(body) = &event.event.body;
    match body.topics.get(1) {
        Some(ScVal::Error(ScError::Contract(code))) => Some(*code),
        _ => None,
    }
}
