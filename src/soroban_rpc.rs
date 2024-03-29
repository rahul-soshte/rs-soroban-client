#![allow(non_snake_case)]
use std::collections::HashMap;
use stellar_baselib::soroban_data_builder::SorobanDataBuilder;

pub mod soroban_rpc {
    use serde::Deserialize;

    use super::*;

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

    #[derive(Clone, Debug, Deserialize)]

    pub struct Cost {
        pub cpu_insns: String,
        pub mem_bytes: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct GetHealtWrapperResponse {
        jsonrpc: String,
        id: u32,
        result: GetHealthResponse,
    }


    #[derive(Debug, Deserialize)]
    pub struct GetHealthResponse {
        pub status: String, // Can be an enum if the number of statuses is known
    }

    #[derive(Clone, Debug, Deserialize)]

    pub struct LedgerEntryResult {
        pub key: String,
        pub xdr: String,
        pub lastModifiedLedgerSeq: Option<i32>,
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

    #[derive(Debug, Clone, Deserialize)]
    pub struct RawLedgerEntryResult {
        pub last_modified_ledger_seq: Option<i64>,
        pub key: String,
        pub xdr: String,
        pub live_until_ledger_seq: Option<i64>,
    }

    #[derive(Debug, Deserialize)]
    pub struct RawGetLedgerEntriesResponse {
        pub entries: Option<Vec<RawLedgerEntryResult>>
        // pub latest_ledger: i32,
    }


    #[derive(Debug, Deserialize)]
    pub struct GetNetworkResponseWrapper {
        pub jsonrpc: String,
        pub id: u32,
        pub result: GetNetworkResponse,
    }


    #[derive(Deserialize, Debug)]
    #[allow(non_snake_case)]
    pub struct GetNetworkResponse {
        pub friendbotUrl: Option<String>,
        pub passphrase: Option<String>,
        pub protocolVersion: Option<i32>,
    }

    #[derive(Deserialize)]
    pub struct GetLatestLedgerResponse {
        pub id: String,
        pub sequence: i32,
        pub protocol_version: String,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[allow(non_camel_case_types)]
    pub enum GetTransactionStatus {
        SUCCESS,
        NOT_FOUND,
        FAILED,
    }

    #[derive(Debug)]
    pub enum GetTransactionResponse {
        Successful(GetSuccessfulTransactionResponse),
        Failed(GetFailedTransactionResponse),
        Missing(GetMissingTransactionResponse),
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct GetAnyTransactionResponse {
        pub status: GetTransactionStatus,
        pub latestLedger: i32,
        pub latestLedgerCloseTime: i32,
        pub oldestLedger: i32,
        pub oldestLedgerCloseTime: i32,
    }

    #[derive(Debug)]
    pub struct GetMissingTransactionResponse {
        pub base: GetAnyTransactionResponse,
    }

    #[derive(Debug)]
    pub struct GetFailedTransactionResponse {
        pub base: GetAnyTransactionResponse,
    }

    #[derive(Clone, Deserialize, Debug)]
    pub struct GetSuccessfulTransactionResponse {
        pub base: Option<GetAnyTransactionResponse>,
        pub ledger: Option<i32>,
        pub createdAt: Option<i32>,
        pub applicationOrder: Option<i32>,
        pub feeBump: Option<bool>,
        pub envelopeXdr: Option<stellar_xdr::next::TransactionEnvelope>,
        pub resultXdr: Option<stellar_xdr::next::TransactionResult>,
        pub resultMetaXdr: Option<stellar_xdr::next::TransactionMeta>,
        pub returnValue: Option<stellar_xdr::next::ScVal>,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct RawGetTransactionResponseWrapper {
        pub jsonrpc: String,
        pub id: i32,
        pub result: RawGetTransactionResponse,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct RawGetTransactionResponse {
        pub status: GetTransactionStatus,
        pub latestLedger: i32,
        pub latestLedgerCloseTime: String,
        pub oldestLedger: i32,
        pub oldestLedgerCloseTime: String,
        pub applicationOrder: Option<i32>,
        pub feeBump: Option<bool>,
        pub envelopeXdr: Option<String>,
        pub resultXdr: Option<String>,
        pub resultMetaXdr: Option<String>,
        pub ledger: Option<i32>,
        pub createdAt: Option<String>,
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

    #[derive(Clone, Debug, Deserialize)]
    #[allow(non_camel_case_types)]
    pub enum SendTransactionStatus {
        PENDING,
        DUPLICATE,
        TRY_AGAIN_LATER,
        ERROR,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct SendTransactionResponse {
        pub status: SendTransactionStatus,
        pub errorResultXdr: Option<String>,
        pub hash: String,
        pub latestLedger: i32,
        pub latestLedgerCloseTime: i32,
    }

    #[derive(Clone, Debug)]
    pub struct SimulateHostFunctionResult {
        pub auth: Vec<stellar_xdr::next::SorobanAuthorizationEntry>,
        pub retval: stellar_xdr::next::ScVal,
    }

    #[derive(Clone, Debug)]

    pub enum SimulateTransactionResponse {
        Success(SimulateTransactionSuccessResponse),
        Restore(SimulateTransactionRestoreResponse),
        Error(SimulateTransactionErrorResponse),
    }

    #[derive(Clone, Debug)]

    pub struct BaseSimulateTransactionResponse {
        pub id: String,
        pub latest_ledger: i32,
        pub events: Vec<stellar_xdr::next::DiagnosticEvent>,
        pub _parsed: bool,
    }

    #[derive(Clone, Debug)]
    pub struct SimulateTransactionSuccessResponse {
        pub base: BaseSimulateTransactionResponse,
        pub latest_ledger: u32,
        pub transaction_data: SorobanDataBuilder,
        pub min_resource_fee: String,
        pub cost: Cost,
        pub result: Option<SimulateHostFunctionResult>,
    }

    #[derive(Clone, Debug)]

    pub struct SimulateTransactionErrorResponse {
        pub base: BaseSimulateTransactionResponse,
        pub error: String,
    }
    #[derive(Clone, Debug)]

    pub struct SimulateTransactionRestoreResponse {
        pub base: SimulateTransactionSuccessResponse,
        // pub result: SimulateHostFunctionResult,
        pub restore_preamble: RestorePreamble,
        pub(crate) result: Option<SimulateHostFunctionResult>,
    }

    #[derive(Clone, Debug, Deserialize)]

    pub struct RestorePreamble {
        pub min_resource_fee: String,
        pub transaction_data: SorobanDataBuilder,
    }

    #[derive(Clone, Deserialize)]
    pub struct RawSimulateHostFunctionResult {
        pub auth: Option<Vec<String>>,
        pub xdr: Option<String>,
    }

    #[derive(Clone, Deserialize)]
    pub struct RawSimulateTransactionResponse {
        pub id: String,
        pub latest_ledger: i32,
        pub error: Option<String>,
        pub transaction_data: Option<String>,
        pub events: Option<Vec<String>>,
        pub min_resource_fee: Option<String>,
        pub results: Option<Vec<RawSimulateHostFunctionResult>>,
        pub cost: Option<Cost>,
        pub restore_preamble: Option<RestorePreamble>,
    }

    pub fn is_simulation_error(sim: &SimulateTransactionResponse) -> bool {
        matches!(sim, SimulateTransactionResponse::Error(_))
    }

    pub fn is_simulation_success(sim: &SimulateTransactionResponse) -> bool {
        matches!(sim, SimulateTransactionResponse::Success(_))
    }

    pub fn is_simulation_restore(sim: &SimulateTransactionResponse) -> bool {
        matches!(sim, SimulateTransactionResponse::Restore(_))
    }
}
