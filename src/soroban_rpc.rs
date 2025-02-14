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

/// Response to [get_health](crate::Server::get_health) RPC method
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetHealthResponse {
    /// Health status, typically 'healthy'
    pub status: String,
    /// Most recent known ledger sequence
    pub latest_ledger: u64,
    /// Oldest ledger sequence kept in history
    pub oldest_ledger: u64,
    /// Maximum retention window configured. A full window state can be determined via:
    /// ledgerRetentionWindow = latestLedger - oldestLedger + 1
    pub ledger_retention_window: u64,
}

/// A pair of LedgerKey and LedgerEntryData
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryResult {
    /// The ledger sequence number of the last time this entry was updated.
    pub last_modified_ledger_seq: Option<i64>,
    /// Sequence number of the ledger.
    pub live_until_ledger_seq: Option<u32>,
    key: String,
    xdr: String,
}

impl LedgerEntryResult {
    /// The key of the ledger entry
    pub fn to_key(&self) -> LedgerKey {
        LedgerKey::from_xdr_base64(&self.key, Limits::none()).expect("Invalid LedgerKey from RPC")
    }
    /// The current value of the given ledger entry
    pub fn to_data(&self) -> LedgerEntryData {
        LedgerEntryData::from_xdr_base64(&self.xdr, Limits::none())
            .expect("Invalid LedgerEntryData from RPC")
    }
}

/// Response to [get_ledger_entries](crate::Server::get_ledger_entries)
#[derive(Deserialize, Debug, Clone)]
pub struct GetLedgerEntriesResponse {
    /// Array of objects containing all found ledger entries
    pub entries: Option<Vec<LedgerEntryResult>>,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latestLedger: i32,
}

/// Response to [get_network](crate::Server::get_network)
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetNetworkResponse {
    /// Network passphrase configured for this Stellar RPC node.
    pub passphrase: Option<String>,
    /// Stellar Core protocol version associated with the latest ledger.
    pub protocol_version: Option<i32>,
    /// (optional) The URL of this network's "friendbot" faucet
    pub friendbot_url: Option<String>,
}

/// Response to [get_latest_ledger](crate::Server::get_latest_ledger)
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetLatestLedgerResponse {
    /// Hash identifier of the latest ledger (as a hex-encoded string) known to Stellar RPC at the time it handled the request.
    pub id: String,
    /// Stellar Core protocol version associated with the latest ledger.
    pub protocol_version: u32,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub sequence: u64,
}

/// Status of [GetTransactionResponse]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GetTransactionStatus {
    /// Transaction succeeded
    Success,
    /// NotFound, may not exist yet
    NotFound,
    /// Transaction failed
    Failed,
}

/// Response to [get_transaction](crate::Server::get_transaction)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionResponse {
    /// The current status of the transaction by hash
    pub status: GetTransactionStatus,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: i32,
    /// The unix timestamp of the close time of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger_close_time: String,
    /// The sequence number of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger: i32,
    /// The unix timestamp of the close time of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger_close_time: String,
    /// (optional) The sequence number of the ledger which included the transaction. This field is only present if status is [GetTransactionStatus::Success] or [GetTransactionStatus::Failed].
    pub ledger: Option<i32>,
    /// (optional) The unix timestamp of when the transaction was included in the ledger. This field is only present if status is [GetTransactionStatus::Success] or [GetTransactionStatus::Failed].
    pub created_at: Option<String>,
    /// (optional) The index of the transaction among all transactions included in the ledger. This field is only present if status is [GetTransactionStatus::Success] or [GetTransactionStatus::Failed].
    pub application_order: Option<i32>,
    /// (optional) Indicates whether the transaction was fee bumped. This field is only present if status is [GetTransactionStatus::Success] or [GetTransactionStatus::Failed].
    pub fee_bump: Option<bool>,
    envelope_xdr: Option<String>,
    result_xdr: Option<String>,
    result_meta_xdr: Option<String>,
}

impl GetTransactionResponse {
    /// (optional) The [TransactionEnvelope] struct for this transaction.
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

    /// (optional) The [TransactionResult] struct for this transaction. This field is only present if status is [GetTransactionStatus::Success] or [GetTransactionStatus::Failed].
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

    /// (optional) The [TransactionMeta] struct of this transaction. Also return the optional
    /// return value of the transaction.
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

/// Set the boundaries while fetching the events
///
/// `From(start) and FromTo(start, end)`
/// `start` is the ledger sequence number to start fetching responses from (inclusive). This
/// method will return an error if startLedger is less than the oldest ledger stored in this node,
/// or greater than the latest ledger seen by this node.
///
/// `end` is the ledger sequence number represents the end of search window (exclusive)
///
/// `Cursor(cursor)`
/// A unique identifier (specifically, a [TOID]) that points to a specific location in a collection
/// of responses and is pulled from the paging_token value of a record. When a cursor is provided,
/// RPC will not include the element whose ID matches the cursor in the response: only elements
/// which appear after the cursor will be included.
///
/// [TOID]: https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0035.md#specification
pub enum EventLedger {
    /// Fetch events starting at this ledger sequence
    From(u64),
    /// Fetch events from and up to these ledger sequences
    FromTo(u64, u64),
    /// Fetch events after this cursor
    Cursor(String),
}

/// Event types (system, contract, or diagnostic) used to filter events
#[derive(PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    /// Only contract type events
    Contract,
    /// Only system type events
    System,
    /// Only diagnostic events
    Diagnostic,
    /// Any event type, contract, system and diagnostic
    All,
}

/// List of filters for the returned events. Events matching any of the filters are included.
/// To match a filter, an event must match both a contractId and a topic. Maximum 5 filters are
/// allowed per request.
pub struct EventFilter {
    event_type: EventType,
    contract_ids: Vec<String>,
    topics: Vec<Vec<Topic>>,
}

/// Topic to match on in the filter
#[derive(Clone, Debug)]
pub enum Topic {
    /// Match the [ScVal]
    Val(ScVal),
    /// Match any `ScVal`
    Any,
}
impl EventFilter {
    /// Start building a new filter for this [EventType]
    pub fn new(event_type: EventType) -> Self {
        EventFilter {
            event_type,
            contract_ids: Vec::new(),
            topics: Vec::new(),
        }
    }

    /// Include this `contract_id` in the filter. If omitted, return events for all contracts.
    /// Maximum 5 contract IDs are allowed per request.
    pub fn contract(self, contract_id: &str) -> Self {
        let mut contract_ids = self.contract_ids.to_vec();
        contract_ids.push(contract_id.to_string());
        EventFilter {
            contract_ids,
            ..self
        }
    }

    /// List of topic filters. If omitted, query for all events. If multiple filters are specified,
    /// events will be included if they match any of the filters. Maximum 5 filters are allowed
    /// per request.
    pub fn topic(self, filer: Vec<Topic>) -> Self {
        let mut topics = self.topics.to_vec();
        topics.push(filer);
        EventFilter { topics, ..self }
    }

    pub(crate) fn event_type(&self) -> Option<String> {
        match self.event_type {
            EventType::Contract => Some("contract".to_string()),
            EventType::System => Some("system".to_string()),
            EventType::Diagnostic => Some("diagnostic".to_string()),
            EventType::All => None,
        }
    }

    pub(crate) fn contracts(&self) -> Vec<String> {
        self.contract_ids.to_vec()
    }

    pub(crate) fn topics(&self) -> Vec<Vec<String>> {
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

/// Response to [get_events](crate::Server::get_events)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEventsResponse {
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: u64,
    /// Events found for the filter
    pub events: Vec<EventResponse>,
    /// The last populated event ID if total events reach the limit or end of the search window.
    pub cursor: Option<String>,
}

/// Event data
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventResponse {
    /// The type of event emission.

    #[serde(rename = "type")]
    pub event_type: EventType,
    /// Sequence number of the ledger in which this event was emitted.
    pub ledger: u64,
    /// ISO-8601 timestamp of the ledger closing time
    pub ledger_closed_at: String,
    /// `StrKey` representation of the contract address that emitted this event.
    pub contract_id: String,
    /// Unique identifier for this event.
    ///
    /// The event's unique id field is based on a toid from Horizon as used in
    /// Horizon's [/effects endpoint](https://github.com/stellar/go/blob/master/services/horizon/internal/db2/history/effect.go#L58).
    ///
    /// Specifically, it is a string containing:
    /// - bigint(32 bit ledger sequence + 20 bit txn number + 12 bit operation) + <hyphen> + number for the event within the operation.
    ///   For example: 1234-1
    pub id: String,
    /// The transaction which triggered this event.
    pub tx_hash: String,
    /// Duplicate of `id` field, but in the standard place for pagination tokens.
    pub paging_token: String,
    /// If true the event was emitted during a successful contract call.
    pub in_successful_contract_call: bool,
    topic: Vec<String>,
    value: String,
}

impl EventResponse {
    /// List containing the topic this event was emitted with.
    pub fn topic(&self) -> Vec<ScVal> {
        self.topic
            .iter()
            .map(|t| ScVal::from_xdr_base64(t, Limits::none()).expect("Invalid XDR from RPC"))
            .collect()
    }

    /// The emitted body value of the event (serialized in a base64 string).
    pub fn value(&self) -> ScVal {
        ScVal::from_xdr_base64(&self.value, Limits::none()).expect("Invalid XDR from RPC")
    }
}

/// Status of the transaction
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SendTransactionStatus {
    /// Transaction submitted
    Pending,
    /// Transaction already submitted
    Duplicate,
    /// Transaction in error
    Error,
    /// Transaction cannot be submitted, try again
    TryAgainLater,
}

/// Response to [send_transaction](crate::Server::send_transaction)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionResponse {
    /// The current status of the transaction by hash.
    pub status: SendTransactionStatus,
    /// Transaction hash (as a hex-encoded string)
    pub hash: String,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: u32,
    /// The unix timestamp of the close time of the latest ledger known to Stellar RPC at the
    /// time it handled the request.
    pub latest_ledger_close_time: String,
    error_result_xdr: Option<String>, // Base64 encoded TransactionResult
    diagnostic_events_xdr: Option<Vec<String>>, // Base64 encoded DiagnosticEvent
}

impl SendTransactionResponse {
    /// (optional) If the transaction status is [SendTransactionStatus::Error], this will be a
    /// TransactionResult struct containing details on why stellar-core rejected the transaction.
    pub fn to_error_result(&self) -> Option<TransactionResult> {
        self.error_result_xdr.as_ref().map(|e| {
            TransactionResult::from_xdr_base64(e, Limits::none()).expect("Invalid XDR from RPC")
        })
    }

    /// (optional) If the transaction status is [SendTransactionStatus::Error], this field may
    /// be present with [Vec<DiagnosticEvent>]. Each [DiagnosticEvent] is containing details on
    /// why stellar-core rejected the transaction.
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
#[serde(rename_all = "camelCase")]
struct RestorePreamble {
    min_resource_fee: String,
    transaction_data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RawSimulateHostFunctionResult {
    auth: Vec<String>,
    xdr: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RawStateChanges {
    #[serde(rename = "type")]
    kind: StateChangeKind,
    key: String,
    before: Option<String>,
    after: Option<String>,
}

/// Indicates if the entry was created (1), updated (2), or deleted (3)
#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
#[serde(rename_all = "lowercase")]
pub enum StateChangeKind {
    /// Entry has been created
    Created = 1,
    /// Entry has been updated
    Updated = 2,
    /// Entry has been deleted
    Deleted = 3,
}

/// On successful simulation of InvokeHostFunction operations, this field will be an array of
/// [LedgerEntry]s before and after simulation occurred. Note that at least one of before or after
/// will be present: before and no after indicates a deletion event, the inverse is a creation
/// event, and both present indicates an update event. Or just check the type.
pub struct StateChange {
    /// Type of change
    pub kind: StateChangeKind,
    /// The [LedgerKey] for this delta
    pub key: LedgerKey,
    /// If present, [LedgerEntry] state prior to simulation
    pub before: Option<LedgerEntry>,
    /// If present, [LedgerEntry] state after simulation
    pub after: Option<LedgerEntry>,
}

/// Response to [simulate_transaction](crate::Server::simulate_transaction)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionResponse {
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: i32,
    /// (optional) Stringified number - Recommended minimum resource fee to add when submitting
    /// the transaction. This fee is to be added on top of the Stellar network fee.
    /// Not present in case of error.
    pub min_resource_fee: Option<String>,
    /// (optional) - This field will include details about why the invoke host function call
    /// failed. Only present if the transaction simulation failed.
    pub error: Option<String>,
    results: Option<Vec<RawSimulateHostFunctionResult>>,
    transaction_data: Option<String>,
    restore_preamble: Option<RestorePreamble>,
    events: Option<Vec<String>>,
    state_changes: Option<Vec<RawStateChanges>>,
}

impl SimulateTransactionResponse {
    /// (optional) - This array will only have one element: the result for the Host Function
    /// invocation.
    /// Only present on successful simulation (i.e. no error) of InvokeHostFunction op
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

    /// (optional) - The recommended Soroban Transaction Data to use when
    /// submitting the simulated transaction. This data contains the refundable fee and resource
    /// usage information such as the ledger footprint and IO access data.
    /// Not present in case of error.
    pub fn to_transaction_data(&self) -> Option<SorobanTransactionData> {
        self.transaction_data.as_ref().map(|data| {
            SorobanDataBuilder::new(Some(stellar_baselib::soroban_data_builder::Either::Left(
                data.to_owned(),
            )))
            .build()
        })
    }

    /// (optional) - It can only be present on successful simulation (i.e. no error) of
    /// InvokeHostFunction operations. If present, it indicates that the simulation detected
    /// archived ledger entries which need to be restored before the submission of the
    /// InvokeHostFunction operation.
    /// The minResourceFee and transactionData fields should be used to submit a transaction
    /// containing a RestoreFootprint operation.
    ///```rust
    /// # use std::rc::Rc;
    /// # use std::cell::RefCell;
    /// # use soroban_client::*;
    /// # use soroban_client::account::Account;
    /// # use soroban_client::address::Address;
    /// # use soroban_client::address::AddressTrait;
    /// # use soroban_client::contract::Contracts;
    /// # use soroban_client::contract::ContractBehavior;
    /// # use soroban_client::network::Networks;
    /// # use soroban_client::network::NetworkPassphrase;
    /// # use soroban_client::xdr::Int128Parts;
    /// # use soroban_client::xdr::ScVal;
    /// # use soroban_client::xdr::int128_helpers::i128_from_pieces;
    /// # use soroban_client::keypair::Keypair;
    /// # use soroban_client::keypair::KeypairBehavior;
    /// # use soroban_client::transaction::AccountBehavior;
    /// # use soroban_client::transaction::TransactionBuilderBehavior;
    /// # use soroban_client::transaction_builder::TransactionBuilder;
    /// # #[tokio::main]
    /// # async fn main() {
    /// #   let rpc = Server::new("https://soroban-testnet.stellar.org", Options::default()).unwrap();
    /// #   let native_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    /// #   let native_sac = Contracts::new(native_id).unwrap();
    /// #   let kp = Keypair::random().unwrap();
    /// #   let account = rpc.request_airdrop(&kp.public_key()).await.unwrap();
    /// #   let source_account = Rc::new(RefCell::new(
    /// #       Account::new(&kp.public_key(), &account.sequence_number()).unwrap(),
    /// #   ));
    /// #   let account_address = Address::new(&kp.public_key()).unwrap();
    /// #   let tx = TransactionBuilder::new(source_account, Networks::testnet(), None)
    /// #       .fee(1000u32)
    /// #       .add_operation(
    /// #           native_sac.call(
    /// #               "balance",
    /// #               Some(vec![account_address.to_sc_val().unwrap()])))
    /// #       .build();
    ///
    ///     let simulation = rpc.simulate_transaction(tx, None).await.unwrap();
    ///     if let Some((min_resource_fee, transaction_data)) =
    ///         simulation.to_restore_transaction_data() {
    ///         // Build a RestoreFootprint transaction
    ///     }
    /// # }
    /// ```
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

    /// (optional) - Array of the events emitted during the
    /// contract invocation. The events are ordered by their emission time.
    /// Only present when simulating of InvokeHostFunction operations,
    /// note that it can be present on error, providing extra context about what failed.
    pub fn to_events(&self) -> Option<Vec<DiagnosticEvent>> {
        if let Some(events) = self.events.as_ref() {
            events
                .iter()
                .map(|e| DiagnosticEvent::from_xdr_base64(e, Limits::none()).ok())
                .collect()
        } else {
            None
        }
    }

    /// (optional) - On successful simulation of InvokeHostFunction operations, this field will be
    /// an array of LedgerEntrys before and after simulation occurred. Note that at least one of
    /// before or after will be present: before and no after indicates a deletion event, the
    /// inverse is a creation event, and both present indicates an update event. Or just check the
    /// type.
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

/// Response to [get_fee_stats](crate::Server::get_fee_stats)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeStatsResponse {
    /// Inclusion fee distribution statistics for Soroban transactions
    pub soroban_inclusion_fee: FeeDistribution,
    /// Fee distribution statistics for Stellar (i.e. non-Soroban) transactions.
    /// Statistics are normalized per operation.
    pub inclusion_fee: FeeDistribution,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled
    /// the request.
    pub latest_ledger: u64,
}

/// Fee distribution
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeDistribution {
    /// Maximum fee
    pub max: String,
    /// Minimum fee
    pub min: String,
    /// Fee value which occurs the most often
    pub mode: String,
    /// 10th nearest-rank fee percentile
    pub p10: String,
    /// 20th nearest-rank fee percentile
    pub p20: String,
    /// 30th nearest-rank fee percentile
    pub p30: String,
    /// 40th nearest-rank fee percentile
    pub p40: String,
    /// 50th nearest-rank fee percentile
    pub p50: String,
    /// 60th nearest-rank fee percentile
    pub p60: String,
    /// 70th nearest-rank fee percentile
    pub p70: String,
    /// 80th nearest-rank fee percentile
    pub p80: String,
    /// 90th nearest-rank fee percentile
    pub p90: String,
    /// 95th nearest-rank fee percentile
    pub p95: String,
    /// 99th nearest-rank fee percentile
    pub p99: String,
    /// How many transactions are part of the distribution
    pub transaction_count: String,
    /// How many consecutive ledgers form the distribution
    pub ledger_count: u64,
}

/// Response to [get_version_info](crate::Server::get_version_info)
#[derive(Debug, Deserialize)]
pub struct GetVersionInfoResponse {
    /// The version of the RPC server.
    pub version: String,
    /// The commit hash of the RPC server.
    pub commit_hash: String,
    /// The build timestamp of the RPC server.
    pub build_time_stamp: String,
    /// The version of the Captive Core.
    pub captive_core_version: String,
    /// The protocol version.
    pub protocol_version: u32,
}
