use stellar_baselib::soroban_data_builder::SorobanDataBuilder;
use std::collections::HashMap;

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

    #[derive(Clone, Debug)]

    pub struct Cost {
        pub cpu_insns: String,
        pub mem_bytes: String,
    }

    #[derive(Deserialize)]
    pub struct GetHealthResponse {
        pub status: String, // Can be an enum if the number of statuses is known
    }

    #[derive(Deserialize)]

    pub struct LedgerEntryResult {
        pub last_modified_ledger_seq: Option<i32>,
        pub key: String,
        pub xdr: String,
    }

    #[derive(Deserialize)]
    pub struct GetLedgerEntriesResponse {
        pub entries: Option<Vec<LedgerEntryResult>>,
        pub latest_ledger: i32,
    }

    pub struct GetNetworkResponse {
        pub friendbot_url: Option<String>,
        pub passphrase: String,
        pub protocol_version: String,
    }

    pub struct GetLatestLedgerResponse {
        pub id: String,
        pub sequence: i32,
        pub protocol_version: String,
    }
    #[allow(non_camel_case_types)]
    pub enum GetTransactionStatus {
        SUCCESS,
        NOT_FOUND,
        FAILED,
    }

    pub enum GetTransactionResponse {
        Successful(GetSuccessfulTransactionResponse),
        Failed(GetFailedTransactionResponse),
        Missing(GetMissingTransactionResponse),
    }

    pub struct GetAnyTransactionResponse {
        pub status: GetTransactionStatus,
        pub latest_ledger: i32,
        pub latest_ledger_close_time: i32,
        pub oldest_ledger: i32,
        pub oldest_ledger_close_time: i32,
    }

    pub struct GetMissingTransactionResponse {
        pub base: GetAnyTransactionResponse,
    }

    pub struct GetFailedTransactionResponse {
        pub base: GetAnyTransactionResponse,
    }

    pub struct GetSuccessfulTransactionResponse {
        pub base: GetAnyTransactionResponse,
        pub ledger: i32,
        pub created_at: i32,
        pub application_order: i32,
        pub fee_bump: bool,
        pub envelope_xdr: stellar_xdr::curr::TransactionEnvelope,
        pub result_xdr: stellar_xdr::curr::TransactionResult,
        pub result_meta_xdr: stellar_xdr::curr::TransactionMeta,
        pub return_value: Option<stellar_xdr::next::ScVal>,
    }

    pub struct RawGetTransactionResponse {
        pub status: GetTransactionStatus,
        pub latest_ledger: i32,
        pub latest_ledger_close_time: i32,
        pub oldest_ledger: i32,
        pub oldest_ledger_close_time: i32,
        pub application_order: Option<i32>,
        pub fee_bump: Option<bool>,
        pub envelope_xdr: Option<String>,
        pub result_xdr: Option<String>,
        pub result_meta_xdr: Option<String>,
        pub ledger: Option<i32>,
        pub created_at: Option<i32>,
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

    #[allow(non_camel_case_types)]
    pub enum SendTransactionStatus {
        PENDING,
        DUPLICATE,
        TRY_AGAIN_LATER,
        ERROR,
    }

    pub struct SendTransactionResponse {
        pub status: SendTransactionStatus,
        pub error_result_xdr: Option<String>,
        pub hash: String,
        pub latest_ledger: i32,
        pub latest_ledger_close_time: i32,
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

    #[derive(Clone, Debug)]

    pub struct RestorePreamble {
        pub min_resource_fee: String,
        pub transaction_data: SorobanDataBuilder,
    }
    
    #[derive(Clone)]
    pub struct RawSimulateHostFunctionResult {
        pub auth: Option<Vec<String>>,
        pub xdr: Option<String>,
    }

    #[derive(Clone)]
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
