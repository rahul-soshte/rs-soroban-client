#![allow(non_snake_case)]
use std::ops::Deref;

use stellar_baselib::{
    soroban_data_builder::{SorobanDataBuilder, SorobanDataBuilderBehavior},
    xdr::{
        ContractEvent, DiagnosticEvent, LedgerCloseMeta, LedgerEntry, LedgerEntryData,
        LedgerHeaderHistoryEntry, LedgerKey, Limits, ReadXdr, ScVal, SorobanAuthorizationEntry,
        SorobanTransactionData, TransactionEnvelope, TransactionEvent, TransactionMeta,
        TransactionResult,
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
    pub latest_ledger: u32,
    /// Oldest ledger sequence kept in history
    pub oldest_ledger: u32,
    /// Maximum retention window configured. A full window state can be determined via:
    /// ledgerRetentionWindow = latestLedger - oldestLedger + 1
    pub ledger_retention_window: u32,
}

/// A pair of LedgerKey and LedgerEntryData
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryResult {
    /// The ledger sequence number of the last time this entry was updated.
    pub last_modified_ledger_seq: Option<u32>,
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
    pub latestLedger: u32,
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
    pub sequence: u32,
}

/// Status of [GetTransactionResponse] or [GetTransactionsResponse] transactions
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionStatus {
    /// Transaction succeeded
    Success,
    /// NotFound, may not exist yet
    NotFound,
    /// Transaction failed
    Failed,
}

/// Response to [get_transaction](crate::Server::get_transaction)
///
/// See [TransactionDetails] for additionnal fields
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionResponse {
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: u32,
    /// The unix timestamp of the close time of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger_close_time: String,
    /// The sequence number of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger: u32,
    /// The unix timestamp of the close time of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger_close_time: String,
    /// (optional) The unix timestamp of when the transaction was included in the ledger. This field is only present if status is [TransactionStatus::Success] or [TransactionStatus::Failed].
    pub created_at: Option<String>,
    /// Transaction details
    #[serde(flatten)]
    transaction: TransactionDetails,
}
// Flatten the transaction in the struct
impl Deref for GetTransactionResponse {
    type Target = TransactionDetails;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
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
    /// Since protocol 23: Diagnostic events are no longer returned by [get_events](crate::Server::get_events)
    Diagnostic,
    /// Any event type, contract, system and diagnostic
    All,
}

/// Response to [get_events](crate::Server::get_events)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEventsResponse {
    /// Events found for the filter
    pub events: Vec<EventResponse>,
    /// The last populated event ID if total events reach the limit or end of the search window.
    pub cursor: Option<String>,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: u64,
    /// The sequence number of the oldest ledger stored in Stellar-RPC
    pub oldest_ledger: Option<u64>,
    /// The unix timestamp of when the latest ledger was closed
    pub latest_ledger_close_time: Option<String>,
    /// The unix timestamp of when the oldest ledger was closed
    pub oldest_ledger_close_time: Option<String>,
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
    /// - bigint(32 bit ledger sequence + 20 bit txn number + 12 bit operation) + &lt;hyphen&gt; + number for the event within the operation.
    ///   For example: 1234-1
    pub id: String,
    /// The index of the operation in the transaction
    pub operation_index: Option<u32>,
    /// The index of the transaction in the ledger
    pub transaction_index: Option<u32>,
    /// The transaction which triggered this event.
    pub tx_hash: String,
    /// Duplicate of `id` field, but in the standard place for pagination tokens.
    /// Since protocol 23: This field is no longer present
    pub paging_token: Option<String>,
    /// If true the event was emitted during a successful contract call.
    ///
    /// Deprecated: will be remove in protocol 24
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
    /// be present with [`Vec<DiagnosticEvent>`]. Each [DiagnosticEvent] is containing details on
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
    pub latest_ledger: u32,
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
    ///     let simulation = rpc.simulate_transaction(&tx, None).await.unwrap();
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
    pub latest_ledger: u32,
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
    pub ledger_count: u32,
}

/// Response to [get_version_info](crate::Server::get_version_info)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVersionInfoResponse {
    /// The version of the RPC server.
    pub version: String,
    /// The commit hash of the RPC server.
    pub commit_hash: String,
    /// The build timestamp of the RPC server.
    pub build_timestamp: String,
    /// The version of the Captive Core.
    pub captive_core_version: String,
    /// The protocol version.
    pub protocol_version: u32,
}

/// Response to [get_transactions](crate::Server::get_transactions)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionsResponse {
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: u32,
    /// The unix timestamp of the close time of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger_close_timestamp: i64,
    /// The sequence number of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger: u32,
    /// The unix timestamp of the close time of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger_close_timestamp: i64,
    /// Cursor reference
    pub cursor: String,
    /// The transactions found
    pub transactions: Vec<TransactionInfo>,
}

/// Representation of a transaction returned by stellar RPC
///
/// Specific type for [GetTransactionsResponse]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
    /// The unix timestamp of when the transaction was included in the ledger.
    pub created_at: Option<i64>,
    #[serde(flatten)]
    transaction: TransactionDetails,
}
// Flatten the transaction in the struct
impl Deref for TransactionInfo {
    type Target = TransactionDetails;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
}

/// Representation of a transaction returned by stellar RPC
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetails {
    /// The current status of the transaction by hash
    pub status: TransactionStatus,
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    /// (optional) The sequence number of the ledger which included the transaction. This field is only present if status is [TransactionStatus::Success] or [TransactionStatus::Failed].
    pub ledger: Option<u32>,
    /// (optional) The index of the transaction among all transactions included in the ledger. This field is only present if status is [TransactionStatus::Success] or [TransactionStatus::Failed].
    pub application_order: Option<i32>,
    /// (optional) Indicates whether the transaction was fee bumped. This field is only present if status is [TransactionStatus::Success] or [TransactionStatus::Failed].
    pub fee_bump: Option<bool>,
    envelope_xdr: Option<String>,
    result_xdr: Option<String>,
    result_meta_xdr: Option<String>,
    diagnostic_events_xdr: Option<Vec<String>>,
    events: Option<Events>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Events {
    transaction_events_xdr: Option<Vec<String>>,
    contract_events_xdr: Option<Vec<Vec<String>>>,
}

impl TransactionDetails {
    /// (optional) The [TransactionEnvelope] struct for this transaction.
    pub fn to_envelope(&self) -> Option<TransactionEnvelope> {
        if let Some(result) = &self.envelope_xdr {
            let r = TransactionEnvelope::from_xdr_base64(result, Limits::none());
            r.ok()
        } else {
            None
        }
    }

    /// (optional) The [TransactionResult] struct for this transaction. This field is only present if status is [TransactionStatus::Success] or [TransactionStatus::Failed].
    pub fn to_result(&self) -> Option<TransactionResult> {
        if let Some(result) = &self.result_xdr {
            let r = TransactionResult::from_xdr_base64(result, Limits::none());
            r.ok()
        } else {
            None
        }
    }

    /// (optional) The [TransactionMeta] struct of this transaction. Also return the optional
    /// return value of the transaction.
    pub fn to_result_meta(&self) -> Option<(TransactionMeta, Option<ScVal>)> {
        if let Some(result) = &self.result_meta_xdr {
            let r = TransactionMeta::from_xdr_base64(result, Limits::none());
            if let Ok(e) = r {
                let mut return_value = None;
                match &e {
                    TransactionMeta::V3(v3) => {
                        if let Some(v) = &v3.soroban_meta {
                            return_value = Some(v.return_value.clone());
                        }
                    }
                    TransactionMeta::V4(v4) => {
                        if let Some(v) = &v4.soroban_meta {
                            return_value = v.return_value.clone();
                        }
                    }
                    _ => {}
                };
                Some((e, return_value))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// (optional) A base64 encoded slice of xdr.DiagnosticEvent. This is only present if the
    /// ENABLE_SOROBAN_DIAGNOSTIC_EVENTS has been enabled in the stellar-core config.
    ///
    /// Deprecated: will be removed in protocol 24
    pub fn to_diagnostic_events(&self) -> Option<Vec<DiagnosticEvent>> {
        if let Some(events) = &self.diagnostic_events_xdr {
            events
                .iter()
                .map(|e| DiagnosticEvent::from_xdr_base64(e, Limits::none()).ok())
                .collect()
        } else {
            None
        }
    }

    /// Events contains all events related to the transaction: transaction and contract events.
    pub fn to_events(&self) -> Option<(Vec<TransactionEvent>, Vec<Vec<ContractEvent>>)> {
        if let Some(events) = &self.events {
            let tx_events = match &events.transaction_events_xdr {
                Some(te) => {
                    let v: Option<Vec<TransactionEvent>> = te
                        .iter()
                        .map(|e| TransactionEvent::from_xdr_base64(e, Limits::none()).ok())
                        .collect();
                    v.unwrap_or_default()
                }
                None => Vec::default(),
            };
            let cx_events = match &events.contract_events_xdr {
                Some(te) => {
                    let v: Option<Vec<Vec<ContractEvent>>> = te
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|e| ContractEvent::from_xdr_base64(e, Limits::none()).ok())
                                .collect()
                        })
                        .collect();
                    v.unwrap_or_default()
                }
                None => Vec::default(),
            };
            Some((tx_events, cx_events))
        } else {
            None
        }
    }
}

/// Response to [get_ledgers](crate::Server::get_ledgers)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetLedgersResponse {
    /// The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger: u32,
    /// The unix timestamp of the close time of the latest ledger known to Stellar RPC at the time it handled the request.
    pub latest_ledger_close_time: i64,
    /// The sequence number of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger: u32,
    /// The unix timestamp of the close time of the oldest ledger ingested by Stellar RPC at the time it handled the request.
    pub oldest_ledger_close_time: i64,
    /// Cursor reference
    pub cursor: String,
    /// Ledgers returned
    pub ledgers: Vec<LedgerInfo>,
}

/// Representation of the ledger
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerInfo {
    /// The hash of the ledger header which was included in the chain
    pub hash: String,
    /// The sequence number of the ledger (sometimes called the 'block height').
    pub sequence: u32,
    /// The timestamp at which the ledger was closed.
    pub ledger_close_time: String,
    header_xdr: Option<String>,
    metadataXdr: Option<String>,
}

impl LedgerInfo {
    /// LedgerHeader for this ledger
    pub fn to_header(&self) -> Option<LedgerHeaderHistoryEntry> {
        self.header_xdr.as_ref().map(|header| {
            LedgerHeaderHistoryEntry::from_xdr_base64(header, Limits::none())
                .expect("Invalid XDR from RPC")
        })
    }
    /// LedgerCloseMeta for this ledger
    pub fn to_metadata(&self) -> Option<LedgerCloseMeta> {
        self.metadataXdr.as_ref().map(|meta| {
            LedgerCloseMeta::from_xdr_base64(meta, Limits::none()).expect("Invalid XDR from RPC")
        })
    }
}
