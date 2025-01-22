#![allow(non_snake_case)]
use std::collections::HashMap;
use stellar_baselib::{
    soroban_data_builder::SorobanDataBuilder,
    xdr::{Limits, ReadXdr, ScVal, TransactionEnvelope, TransactionMeta, TransactionResult},
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

pub struct Cost {
    pub cpu_insns: String,
    pub mem_bytes: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct GetHealthWrapperResponse {
    pub jsonrpc: String,
    pub id: u32,
    pub result: GetHealthResponse,
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
    pub key: String,
    pub xdr: String,
    pub last_modified_ledger_seq: Option<i64>,
    pub live_until_ledger_seq: Option<u32>,
}

#[derive(Deserialize, Debug)]
pub struct GetLedgerEntriesResponse {
    pub entries: Option<Vec<LedgerEntryResult>>,
    pub latestLedger: i32,
}

#[derive(Deserialize, Debug)]
pub struct GetLedgerEntriesResponseWrapper {
    pub jsonrpc: String,
    pub id: u32,
    pub result: GetLedgerEntriesResponse,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct GetNetworkResponseWrapper {
    pub jsonrpc: String,
    pub id: u32,
    pub result: GetNetworkResponse,
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
#[allow(non_camel_case_types)]
pub enum GetTransactionStatus {
    SUCCESS,
    NOT_FOUND,
    FAILED,
}

#[derive(Debug, Deserialize)]
pub struct GetTransactionResponseWrapper {
    pub jsonrpc: String,
    pub id: u32,
    pub result: GetTransactionResponse,
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

pub enum EventType {
    Contract,
    System,
    Diagnostic,
}

pub struct EventFilter {
    pub event_type: Option<EventType>,
    pub contract_ids: Option<Vec<String>>,
    pub topics: Option<Vec<Vec<String>>>,
}

pub struct GetEventsResponse {
    pub latest_ledger: i32,
    pub events: Vec<EventResponse>,
}

pub struct EventResponse {
    pub event_type: EventType,
    pub ledger: String,
    pub ledger_closed_at: String,
    pub contract_id: String,
    pub id: String,
    pub paging_token: String,
    pub in_successful_contract_call: bool,
    pub topic: Vec<String>,
    pub value: HashMap<String, String>, // Assuming this to be a key-value pair, need to update depending on structure
}

pub struct RequestAirdropResponse {
    pub transaction_id: String,
}

// #[derive(Clone, Debug, Deserialize)]
// #[allow(non_camel_case_types)]
// pub enum SendTransactionStatus {
//     PENDING,
//     DUPLICATE,
//     TRY_AGAIN_LATER,
//     ERROR,
// }

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SendTransactionStatus {
    Pending,
    Duplicate,
    Error,
    Success,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionResponse {
    pub status: SendTransactionStatus,
    pub hash: String,
    pub latest_ledger: u32,
    pub latest_ledger_close_time: String,
    pub error_result_xdr: Option<String>, // Base64 encoded TransactionResult
    pub diagnostic_events_xdr: Option<Vec<String>>, // Base64 encoded DiagnosticEvent
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
    pub transaction_data: SorobanDataBuilder,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawSimulateHostFunctionResult {
    pub auth: Vec<String>,
    pub xdr: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulateTransactionResponseWrapper {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: SimulateTransactionResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionResponse {
    pub latest_ledger: i32,
    pub min_resource_fee: Option<String>,
    pub results: Option<Vec<RawSimulateHostFunctionResult>>,
    pub transaction_data: Option<String>,
    pub events: Option<Vec<String>>,
    pub restore_preamble: Option<RestorePreamble>,
    pub error: Option<String>,
    pub cost: Option<Cost>,
}
