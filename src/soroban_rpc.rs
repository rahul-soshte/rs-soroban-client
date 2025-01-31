#![allow(non_snake_case)]
use stellar_baselib::{
    soroban_data_builder::{SorobanDataBuilder, SorobanDataBuilderBehavior},
    xdr::{
        DiagnosticEvent, LedgerEntry, LedgerEntryData, LedgerKey, Limits, ReadXdr, ScVal,
        SorobanAuthorizationEntry, SorobanTransactionData, TransactionEnvelope, TransactionMeta,
        TransactionResult, WriteXdr,
    },
};

use serde::{Deserialize, Serialize};

pub enum AssetTypeEnum {
    Credit4,
    Credit12,
}

pub struct Balance {
    pub asset_type: AssetTypeEnum,
    pub asset_code: String,
    pub asset_issuer: String,
    pub classic: String,
    pub smart: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub cpu_insns: String,
    pub mem_bytes: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct GetHealthResponse {
    pub status: String, // Can be an enum if the number of statuses is known
                        /*
                        pub latestLedger: u32,
                        pub oldestLedger: u32,
                        pub ledgerRetentionWindow: u32,
                        */
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryResult {
    pub last_modified_ledger_seq: Option<i64>,
    pub live_until_ledger_seq: Option<u32>,
    key: String,
    xdr: String,
}

impl LedgerEntryResult {
    pub fn to_key(&self) -> LedgerKey {
        LedgerKey::from_xdr_base64(&self.key, Limits::none()).expect("Invalid LedgerKey from RPC")
    }
    pub fn to_data(&self) -> LedgerEntryData {
        LedgerEntryData::from_xdr_base64(&self.xdr, Limits::none())
            .expect("Invalid LedgerEntryData from RPC")
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct GetLedgerEntriesResponse {
    pub entries: Option<Vec<LedgerEntryResult>>,
    pub latestLedger: i32,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetNetworkResponse {
    pub friendbot_url: Option<String>,
    pub passphrase: Option<String>,
    pub protocol_version: Option<i32>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLatestLedgerResponse {
    pub id: String,
    pub sequence: i32,
    pub protocol_version: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GetTransactionStatus {
    Success,
    NotFound,
    Failed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionResponse {
    pub status: GetTransactionStatus,
    pub latest_ledger: i32,
    pub latest_ledger_close_time: String,
    pub oldest_ledger: i32,
    pub oldest_ledger_close_time: String,
    pub application_order: Option<i32>,
    pub fee_bump: Option<bool>,
    pub ledger: Option<i32>,
    pub created_at: Option<String>,
    envelope_xdr: Option<String>,
    result_xdr: Option<String>,
    result_meta_xdr: Option<String>,
}

impl GetTransactionResponse {
    pub fn get_envelope(&self) -> Option<TransactionEnvelope> {
        if let Some(result) = &self.envelope_xdr {
            let r = TransactionEnvelope::from_xdr_base64(result, Limits::none());
            if let Ok(e) = r {
                Some(e)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_result(&self) -> Option<TransactionResult> {
        if let Some(result) = &self.result_xdr {
            let r = TransactionResult::from_xdr_base64(result, Limits::none());
            if let Ok(e) = r {
                Some(e)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_result_meta(&self) -> Option<(TransactionMeta, Option<ScVal>)> {
        if let Some(result) = &self.result_meta_xdr {
            let r = TransactionMeta::from_xdr_base64(result, Limits::none());
            if let Ok(e) = r {
                let mut return_value = None;
                if let TransactionMeta::V3(v3) = &e {
                    if let Some(v) = &v3.soroban_meta {
                        return_value = Some(v.return_value.clone());
                    }
                }
                Some((e, return_value))
            } else {
                None
            }
        } else {
            None
        }
    }
}

pub enum EventLedger {
    From(u64),
    FromTo(u64, u64),
    Cursor(String),
}

#[derive(PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Contract,
    System,
    Diagnostic,
    All,
}

pub struct EventFilter {
    event_type: EventType,
    contract_ids: Vec<String>,
    topics: Vec<Vec<Topic>>,
}

#[derive(Clone, Debug)]
pub enum Topic {
    Val(ScVal),
    Any,
}
impl EventFilter {
    pub fn new(event_type: EventType) -> Self {
        EventFilter {
            event_type,
            contract_ids: Vec::new(),
            topics: Vec::new(),
        }
    }

    pub fn contract(self, contract_id: &str) -> Self {
        let mut contract_ids = self.contract_ids.to_vec();
        contract_ids.push(contract_id.to_string());
        EventFilter {
            contract_ids,
            ..self
        }
    }

    pub fn topic(self, filer: Vec<Topic>) -> Self {
        let mut topics = self.topics.to_vec();
        topics.push(filer);
        EventFilter { topics, ..self }
    }

    pub fn event_type(&self) -> Option<String> {
        match self.event_type {
            EventType::Contract => Some("contract".to_string()),
            EventType::System => Some("system".to_string()),
            EventType::Diagnostic => Some("diagnostic".to_string()),
            EventType::All => None,
        }
    }

    pub fn contracts(&self) -> Vec<String> {
        self.contract_ids.to_vec()
    }

    pub fn topics(&self) -> Vec<Vec<String>> {
        self.topics
            .iter()
            .map(|v| {
                v.iter()
                    .map(|vv| match vv {
                        Topic::Val(sc_val) => sc_val
                            .to_xdr_base64(Limits::none())
                            .expect("ScVal cannot be converted to base64"),
                        Topic::Any => "*".to_string(),
                    })
                    .collect()
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEventsResponse {
    pub latest_ledger: u64,
    pub events: Vec<EventResponse>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventResponse {
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub ledger: u64,
    pub ledger_closed_at: String,
    pub contract_id: String,
    pub id: String,
    pub tx_hash: String,
    pub paging_token: String,
    pub in_successful_contract_call: bool,
    topic: Vec<String>,
    value: String,
}

impl EventResponse {
    pub fn topic(&self) -> Vec<ScVal> {
        self.topic
            .iter()
            .map(|t| ScVal::from_xdr_base64(t, Limits::none()).expect("Invalid XDR from RPC"))
            .collect()
    }

    pub fn value(&self) -> ScVal {
        ScVal::from_xdr_base64(&self.value, Limits::none()).expect("Invalid XDR from RPC")
    }
}

pub struct RequestAirdropResponse {
    pub transaction_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SendTransactionStatus {
    Pending,
    Duplicate,
    Error,
    TryAgainLater,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionResponse {
    pub status: SendTransactionStatus,
    pub hash: String,
    pub latest_ledger: u32,
    pub latest_ledger_close_time: String,
    error_result_xdr: Option<String>, // Base64 encoded TransactionResult
    diagnostic_events_xdr: Option<Vec<String>>, // Base64 encoded DiagnosticEvent
}

impl SendTransactionResponse {
    pub fn to_error_result(&self) -> Option<TransactionResult> {
        self.error_result_xdr.as_ref().map(|e| {
            TransactionResult::from_xdr_base64(e, Limits::none()).expect("Invalid XDR from RPC")
        })
    }
    pub fn to_diagnostic_events(&self) -> Option<Vec<DiagnosticEvent>> {
        if let Some(events) = self.diagnostic_events_xdr.as_ref() {
            events
                .iter()
                .map(|e| DiagnosticEvent::from_xdr_base64(e, Limits::none()).ok())
                .collect()
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimulateHostFunctionResult {
    pub auth: Vec<stellar_baselib::xdr::SorobanAuthorizationEntry>,
    pub retval: stellar_baselib::xdr::ScVal,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: SendTransactionResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreamble {
    pub min_resource_fee: String,
    pub transaction_data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawSimulateHostFunctionResult {
    pub auth: Vec<String>,
    pub xdr: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
#[serde(rename_all = "lowercase")]
pub enum StateChangeKind {
    Create = 1,
    Updated = 2,
    Deleted = 3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RawStateChanges {
    #[serde(rename = "type")]
    kind: StateChangeKind,
    key: String,
    before: Option<String>,
    after: Option<String>,
}

pub struct StateChange {
    pub kind: StateChangeKind,
    pub key: LedgerKey,
    pub before: Option<LedgerEntry>,
    pub after: Option<LedgerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLeeway {
    pub cpu_instructions: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionResponse {
    pub latest_ledger: i32,
    pub min_resource_fee: Option<String>,
    pub error: Option<String>,
    pub cost: Option<Cost>,
    results: Option<Vec<RawSimulateHostFunctionResult>>,
    transaction_data: Option<String>,
    restore_preamble: Option<RestorePreamble>,
    events: Option<Vec<String>>,
    state_changes: Option<Vec<RawStateChanges>>,
}

impl SimulateTransactionResponse {
    pub fn to_result(&self) -> Option<(ScVal, Vec<SorobanAuthorizationEntry>)> {
        if let Some(r) = self.results.as_ref() {
            let auth: Vec<SorobanAuthorizationEntry> = r[0]
                .auth
                .iter()
                .map(|e| SorobanAuthorizationEntry::from_xdr_base64(e, Limits::none()).unwrap())
                .collect();
            let ret_val = ScVal::from_xdr_base64(&r[0].xdr, Limits::none())
                .expect("Xdr from RPC should be valid");

            Some((ret_val, auth))
        } else {
            None
        }
    }

    pub fn to_transaction_data(&self) -> Option<SorobanTransactionData> {
        self.transaction_data.as_ref().map(|data| {
            SorobanDataBuilder::new(Some(stellar_baselib::soroban_data_builder::Either::Left(
                data.to_owned(),
            )))
            .build()
        })
    }

    pub fn to_restore_transaction_data(&self) -> Option<(i64, SorobanTransactionData)> {
        if let Some(restore) = self.restore_preamble.clone() {
            Some((
                restore
                    .min_resource_fee
                    .parse()
                    .expect("Invalid i64 for min_resource_fee"),
                SorobanDataBuilder::new(Some(stellar_baselib::soroban_data_builder::Either::Left(
                    restore.transaction_data,
                )))
                .build(),
            ))
        } else {
            None
        }
    }

    pub fn to_diagnostic_events(&self) -> Option<Vec<DiagnosticEvent>> {
        if let Some(events) = self.events.as_ref() {
            events
                .iter()
                .map(|e| DiagnosticEvent::from_xdr_base64(e, Limits::none()).ok())
                .collect()
        } else {
            None
        }
    }

    pub fn to_state_changes(&self) -> Vec<StateChange> {
        if let Some(changes) = self.state_changes.as_ref() {
            changes
                .iter()
                .map(|c| StateChange {
                    kind: c.kind,
                    key: LedgerKey::from_xdr_base64(&c.key, Limits::none())
                        .expect("Invalid LedgerKey"),
                    before: c.before.as_ref().map(|e| {
                        LedgerEntry::from_xdr_base64(e, Limits::none())
                            .expect("Invalid LedgerEntry")
                    }),
                    after: c.after.as_ref().map(|e| {
                        LedgerEntry::from_xdr_base64(e, Limits::none())
                            .expect("Invalid LedgerEntry")
                    }),
                })
                .collect()
        } else {
            Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeStatsResponse {
    pub soroban_inclusion_fee: InclusionFee,
    pub inclusion_fee: InclusionFee,
    pub latest_ledger: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InclusionFee {
    pub max: String,
    pub min: String,
    pub mode: String,
    pub p10: String,
    pub p20: String,
    pub p30: String,
    pub p40: String,
    pub p50: String,
    pub p60: String,
    pub p70: String,
    pub p80: String,
    pub p90: String,
    pub p95: String,
    pub p99: String,
    pub transaction_count: String,
    pub ledger_count: u64,
}

#[derive(Debug, Deserialize)]
pub struct GetVersionInfoResponse {
    pub version: String,
    pub commit_hash: String,
    pub build_time_stamp: String,
    pub captive_core_version: String,
    pub protocol_version: u32,
}
