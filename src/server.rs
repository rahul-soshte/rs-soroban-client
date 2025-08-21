use crate::jsonrpc::{JsonRpc, Response};
use crate::soroban_rpc::*;
use crate::transaction::assemble_transaction;
use crate::{error::*, friendbot};
use futures::TryFutureExt;
use serde_json::json;
use std::option::Option;
use std::{collections::HashMap, str::FromStr};
use stellar_baselib::account::Account;
use stellar_baselib::account::AccountBehavior;
use stellar_baselib::address::{Address, AddressTrait};
use stellar_baselib::keypair::KeypairBehavior;
use stellar_baselib::transaction::{Transaction, TransactionBehavior};
use stellar_baselib::xdr::{
    ContractDataDurability, LedgerEntryData, LedgerKey, LedgerKeyAccount, LedgerKeyContractData,
    Limits, ScVal, WriteXdr,
};

/// The default transaction submission timeout for RPC requests, in milliseconds.
pub const SUBMIT_TRANSACTION_TIMEOUT: u32 = 60 * 1000;

/// Representation of the ledger entry durability to be used with [Server::get_contract_data]
#[derive(Debug, PartialEq, Eq)]
pub enum Durability {
    /// Temporary storage, cannot be restored
    Temporary,
    /// Persistent storage, archived when the TTL is expired, can be restored
    Persistent,
}

impl Durability {
    fn to_xdr(&self) -> ContractDataDurability {
        match self {
            Durability::Temporary => ContractDataDurability::Temporary,
            Durability::Persistent => ContractDataDurability::Persistent,
        }
    }
}

/// Set the boundaries while fetching data from the RPC
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
pub enum Pagination {
    /// Fetch events starting at this ledger sequence
    From(u32),
    /// Fetch events from and up to these ledger sequences
    FromTo(u32, u32),
    /// Fetch events after this cursor
    Cursor(String),
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
    /// Match this topic `ScVal`
    Val(ScVal),
    /// Match any topic
    Any,
    /// Match any topic including more topics (can only be the last [Topic])
    Greedy,
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

    fn event_type(&self) -> Option<String> {
        match self.event_type {
            EventType::Contract => Some("contract".to_string()),
            EventType::System => Some("system".to_string()),
            EventType::Diagnostic => Some("diagnostic".to_string()),
            EventType::All => None,
        }
    }

    fn contracts(&self) -> Vec<String> {
        self.contract_ids.to_vec()
    }

    fn topics(&self) -> Vec<Vec<String>> {
        self.topics
            .iter()
            .map(|v| {
                v.iter()
                    .map(|vv| match vv {
                        Topic::Val(sc_val) => sc_val
                            .to_xdr_base64(Limits::none())
                            .expect("ScVal cannot be converted to base64"),
                        Topic::Any => "*".to_string(),
                        Topic::Greedy => "**".to_string(),
                    })
                    .collect()
            })
            .collect()
    }
}

/// Contains configuration for how resources will be calculated when simulating transactions.
#[derive(Debug, Clone)]
pub struct ResourceLeeway {
    /// Allow this many extra instructions when budgeting resources.
    pub cpu_instructions: u64,
}

/// Additionnal options
#[derive(Debug)]
pub struct Options {
    /// If true, using a non HTTPS RPC will not throw an error
    pub allow_http: bool,
    /// Timeout in seconds (default: 10)
    pub timeout: u64,
    /// Additionnal headers to use while requesting the RPC
    pub headers: HashMap<String, String>,
    /// Optional friendbot URL
    pub friendbot_url: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            allow_http: false,
            timeout: 10,
            headers: Default::default(),
            friendbot_url: None,
        }
    }
}

/// The main struct to use to interact with the stellar RPC
#[derive(Debug)]
pub struct Server {
    client: JsonRpc,
    friendbot_url: Option<String>,
}

impl Server {
    /// # Instantiate a new [Server]
    ///
    /// ```rust
    /// use soroban_client::*;
    /// let rpc = Server::new("https://soroban-testnet.stellar.org", Options::default());
    /// ```
    pub fn new(server_url: &str, opts: Options) -> Result<Self, Error> {
        let server_url = reqwest::Url::from_str(server_url)
            .map_err(|_e| Error::InvalidRpc(InvalidRpcUrl::InvalidUri))?;
        let allow_http = opts.allow_http;
        match server_url.scheme() {
            "https" => {
                // good
            }
            "http" if allow_http => {
                // good
            }
            "http" if !allow_http => {
                return Err(Error::InvalidRpc(InvalidRpcUrl::UnsecureHttpNotAllowed));
            }
            _ => {
                return Err(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme));
            }
        };

        Ok(Server {
            client: JsonRpc::new(server_url, opts.timeout, opts.headers),
            friendbot_url: opts.friendbot_url,
        })
    }

    // RPC method implementations -------------------------------

    /// # Call to RPC method [getEvents]
    ///
    /// Clients can request a filtered list of events emitted by a given ledger range.
    ///
    /// Stellar-RPC will support querying within a maximum 7 days of recent ledgers.
    ///
    /// Note, this could be used by the client to only prompt a refresh when there is a new ledger
    /// with relevant events. It should also be used by backend Dapp components to "ingest" events
    /// into their own database for querying and serving.
    ///
    /// If making multiple requests, clients should deduplicate any events received, based on the
    /// event's unique id field. This prevents double-processing in the case of duplicate events
    /// being received.
    ///
    /// By default stellar-rpc retains the most recent 24 hours of events.
    ///
    /// # Example
    /// ```rust
    /// // Fetch 12 events from ledger 67000 for contract "CAA..."
    /// # use soroban_client::soroban_rpc::*;
    /// # use soroban_client::*;
    /// # use soroban_client::error::Error;
    /// # async fn events() -> Result<(), Error> {
    /// # let server = Server::new("https://rpc.server", Options::default())?;
    /// let events = server.get_events(
    ///     Pagination::From(67000),
    ///     vec![
    ///         EventFilter::new(EventType::All).contract("CAA...")
    ///     ],
    ///     12
    /// ).await?;
    /// # return Ok(()); }
    ///
    /// ```
    ///
    /// [getEvents]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getEvents
    ///
    pub async fn get_events(
        &self,
        ledger: Pagination,
        filters: Vec<EventFilter>,
        limit: impl Into<Option<u32>>,
    ) -> Result<GetEventsResponse, Error> {
        let (start_ledger, end_ledger, cursor) = match ledger {
            Pagination::From(s) => (Some(s), None, None),
            Pagination::FromTo(s, e) => (Some(s), Some(e), None),
            Pagination::Cursor(c) => (None, None, Some(c)),
        };
        let filters = filters
            .into_iter()
            .map(|v| {
                //
                json!({
                    "type": v.event_type(),
                    "contractIds": v.contracts(),
                    "topics": v.topics(),
                })
            })
            .collect::<Vec<serde_json::Value>>();

        let params = json!(
        {
            "startLedger": start_ledger,
            "endLedger": end_ledger,
            "filters": filters,
            "pagination": {
                "cursor": cursor,
                "limit": limit.into()
            }
        }
        );

        let response = self.client.post("getEvents", params).await?;
        handle_response(response)
    }

    /// # Call to RPC method [getFeeStats]
    ///
    /// Statistics for charged inclusion fees. The inclusion fee statistics are calculated from
    /// the inclusion fees that were paid for the transactions to be included onto the ledger. For
    /// Soroban transactions and Stellar transactions, they each have their own inclusion fees and
    /// own surge pricing. Inclusion fees are used to prevent spam and prioritize transactions
    /// during network traffic surge.
    ///
    /// [getFeeStats]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getFeeStats
    ///
    pub async fn get_fee_stats(&self) -> Result<GetFeeStatsResponse, Error> {
        let response = self
            .client
            .post("getFeeStats", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    /// # Call to RPC method [getHealth]
    ///
    /// General node health check.
    ///
    /// [getHealth]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getHealth
    ///
    pub async fn get_health(&self) -> Result<GetHealthResponse, Error> {
        let response = self
            .client
            .post("getHealth", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    /// # Call to RPC method [getLatestLedger]
    ///
    /// For finding out the current latest known ledger of this node. This is a subset of the
    /// ledger info from Horizon.
    ///
    /// [getLatestLedger]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getLatestLedger
    ///
    pub async fn get_latest_ledger(&self) -> Result<GetLatestLedgerResponse, Error> {
        let response = self
            .client
            .post("getLatestLedger", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    /// # Call to RPC method [getLedgerEntries]
    ///
    /// For reading the current value of ledger entries directly.
    ///
    /// This method enables the retrieval of various ledger states, such as accounts, trustlines,
    /// offers, data, claimable balances, and liquidity pools. It also provides direct access to
    /// inspect a contract's current state, its code, or any other ledger entry. This serves as a
    /// primary method to access your contract data which may not be available via
    /// [events][Server::get_events] or
    /// [simulate_transaction][Server::simulate_transaction].
    ///
    /// To fetch contract wasm byte-code, use the ContractCode ledger entry key.
    ///
    /// [getLedgerEntries]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getLedgerEntries
    ///
    pub async fn get_ledger_entries(
        &self,
        keys: Vec<LedgerKey>,
    ) -> Result<GetLedgerEntriesResponse, Error> {
        let keys: Result<Vec<String>, Error> = keys
            .into_iter()
            .map(|k| k.to_xdr_base64(Limits::none()).map_err(|_| Error::XdrError))
            .collect();

        match keys {
            Ok(keys) => {
                let params = json!({"keys": keys});
                let response: Response<GetLedgerEntriesResponse> =
                    self.client.post("getLedgerEntries", params).await?;

                handle_response(response)
            }
            Err(err) => Err(err),
        }
    }

    /// # Call to RPC method [getLedgers]
    ///
    /// The getLedgers method returns a detailed list of ledgers starting from the user specified
    /// starting point that you can paginate as long as the pages fall within the history
    /// retention of their corresponding RPC provider.
    ///
    /// [getLedgers]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getLedgers
    pub async fn get_ledgers(
        &self,
        ledger: Pagination,
        limit: impl Into<Option<u32>>,
    ) -> Result<GetLedgersResponse, Error> {
        let (start_ledger, cursor) = match ledger {
            Pagination::From(s) => (Some(s), None),
            Pagination::FromTo(s, _) => (Some(s), None),
            Pagination::Cursor(c) => (None, Some(c)),
        };
        let params = json!(
        {
            "startLedger": start_ledger,
            "pagination": {
                "cursor": cursor,
                "limit": limit.into()
            }
        }
        );

        let response = self.client.post("getLedgers", params).await?;
        handle_response(response)
    }

    /// # Call to RPC method [getNetwork]
    ///
    /// General information about the currently configured network. This response will contain all
    /// the information needed to successfully submit transactions to the network this node serves.
    ///
    /// [getNetwork]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getNetwork
    pub async fn get_network(&self) -> Result<GetNetworkResponse, Error> {
        let response = self
            .client
            .post("getNetwork", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    /// # Call to RPC method [getTransaction]
    ///
    /// The getTransaction method provides details about the specified transaction.
    ///
    /// Clients are expected to periodically query this method to ascertain when a transaction has
    /// been successfully recorded on the blockchain. The stellar-rpc system maintains a restricted
    /// history of recently processed transactions, with the default retention window set at 24
    /// hours.
    ///
    /// For private soroban-rpc instances, it is possible to modify the retention window
    /// value by adjusting the transaction-retention-window configuration setting, but we do not
    /// recommend values longer than 7 days. For debugging needs that extend beyond this timeframe,
    /// it is advisable to index this data yourself, employ a third-party indexer, or query Hubble
    /// (our public BigQuery data set).
    ///
    /// [getTransaction]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getTransaction
    ///
    pub async fn get_transaction(&self, hash: &str) -> Result<GetTransactionResponse, Error> {
        let params = json!({
                "hash": hash
        });

        let response = self.client.post("getTransaction", params).await?;
        handle_response(response)
    }

    /// # Call to RPC method [getTransactions]
    ///
    /// The getTransactions method return a detailed list of transactions starting from the user
    /// specified starting point that you can paginate as long as the pages fall within the
    /// history retention of their corresponding RPC provider.
    ///
    /// In [Pagination::FromTo(start, end)], the `end` has no effect for `get_transactions`.
    ///
    /// [getTransactions]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getTransactions
    pub async fn get_transactions(
        &self,
        ledger: Pagination,
        limit: impl Into<Option<u32>>,
    ) -> Result<GetTransactionsResponse, Error> {
        let (start_ledger, cursor) = match ledger {
            Pagination::From(s) => (Some(s), None),
            Pagination::FromTo(s, _) => (Some(s), None),
            Pagination::Cursor(c) => (None, Some(c)),
        };
        let params = json!(
        {
            "startLedger": start_ledger,
            "pagination": {
                "cursor": cursor,
                "limit": limit.into()
            }
        }
        );

        let response = self.client.post("getTransactions", params).await?;
        handle_response(response)
    }

    /// # Call to RPC method [getVersionInfo]
    ///
    /// Version information about the RPC and Captive core. RPC manages its own, pared-down
    /// version of Stellar Core optimized for its own subset of needs. we'll refer to this as
    /// a "Captive Core" instance.
    ///
    /// [getVersionInfo]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getVersionInfo
    pub async fn get_version_info(&self) -> Result<GetVersionInfoResponse, Error> {
        let response = self
            .client
            .post("getVersionInfo", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    /// # Call to RPC method [sendTransaction]
    ///
    /// Submit a real transaction to the Stellar network. This is the only way to make changes
    /// on-chain.
    ///
    /// Unlike Horizon, this does not wait for transaction completion. It simply validates and
    /// enqueues the transaction. Clients should call getTransaction to learn about transaction
    /// success/failure.
    ///
    /// This supports all transactions, not only smart contract-related transactions.
    ///
    /// [sendTransaction]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/sendTransaction
    ///
    pub async fn send_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<SendTransactionResponse, Error> {
        let transaction_xdr = transaction
            .to_envelope()
            .map_err(|_| Error::TransactionError)?
            .to_xdr_base64(Limits::none())
            .map_err(|_| Error::XdrError)?;

        let params = json!({
                "transaction": transaction_xdr
            }
        );
        let response = self.client.post("sendTransaction", params).await?;
        handle_response(response)
    }

    /// # Call to RPC method [simulateTransaction]
    ///
    /// Submit a trial contract invocation to simulate how it would be executed by the network.
    /// This endpoint calculates the effective transaction data, required authorizations, and
    /// minimal resource fee. It provides a way to test and analyze the potential outcomes of a
    /// transaction without actually submitting it to the network.
    ///
    /// [simulateTransaction]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/simulateTransaction
    pub async fn simulate_transaction(
        &self,
        transaction: Transaction,
        addl_resources: Option<ResourceLeeway>,
    ) -> Result<SimulateTransactionResponse, Error> {
        let transaction_xdr = transaction
            .to_envelope()
            .map_err(|_| Error::TransactionError)?
            .to_xdr_base64(Limits::none())
            .map_err(|_| Error::XdrError)?;

        // Add resource config if provided
        let params = if let Some(resources) = addl_resources {
            json!({
                "transaction": transaction_xdr,
                "resourceConfig": {
                    "instructionLeeway": resources.cpu_instructions
                }
            })
        } else {
            json!({
                "transaction": transaction_xdr
            })
        };

        let response = self.client.post("simulateTransaction", params).await?;
        handle_response(response)
    }

    // Non-RPC method implementations -------------------------------

    /// # Fetch an [Account] to be used to build a transaction
    ///
    /// It uses [Server::get_ledger_entries] to fetch the [LedgerKey::Account]
    ///
    pub async fn get_account(&self, address: &str) -> Result<Account, Error> {
        let account_id = stellar_baselib::keypair::Keypair::from_public_key(address)
            .map_err(|_| Error::AccountNotFound)?
            .xdr_account_id();
        let ledger_key = LedgerKey::Account(LedgerKeyAccount { account_id });

        let resp = self.get_ledger_entries(vec![ledger_key]).await?;
        let entries = resp.entries.unwrap_or_default();
        if entries.is_empty() {
            return Err(Error::AccountNotFound);
        }

        if let LedgerEntryData::Account(account_entry) = entries[0].to_data() {
            Ok(Account::new(address, &account_entry.seq_num.0.to_string()).unwrap())
        } else {
            Err(Error::AccountNotFound)
        }
    }

    /// # Fech the ledger entry specified by the key of the contract
    ///
    /// This can be used to inspect the contract state without using [Server::simulate_transaction]
    /// or to fetch data not available otherwise.
    ///
    pub async fn get_contract_data(
        &self,
        contract: &str,
        key: ScVal,
        durability: Durability,
    ) -> Result<LedgerEntryResult, Error> {
        let sc_address = Address::new(contract)
            .map_err(|_| Error::ContractDataNotFound)?
            .to_sc_address()
            .map_err(|_| Error::ContractDataNotFound)?;

        let contract_key = LedgerKey::ContractData(LedgerKeyContractData {
            key: key.clone(),
            contract: sc_address,
            durability: durability.to_xdr(),
        });

        let val = vec![contract_key];

        let response = self.get_ledger_entries(val).await?;

        if let Some(entries) = response.entries {
            if let Some(entry) = entries.first() {
                Ok(entry.clone())
            } else {
                Err(Error::ContractDataNotFound)
            }
        } else {
            Err(Error::ContractDataNotFound)
        }
    }

    /// # Prepare a transaction to be submited to the network.
    ///
    /// If the transaction simulation is successful, a new transaction is built using the returned
    /// footprint and authorizations.
    ///
    /// The fees are adapted based on the initial fees and the contract resource fees estimated
    /// from the simulation.
    ///
    /// If the simulation returns a restore preamble, this method will return a [Error::RestorationRequired].
    /// This error should be used to build a
    /// [stellar_baselib::xdr::OperationBody::RestoreFootprint]
    ///
    pub async fn prepare_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<Transaction, Error> {
        let sim_response = self.simulate_transaction(transaction.clone(), None).await?;

        assemble_transaction(transaction, sim_response)
    }

    /// # Fund the account using the network's [friendbot] faucet (testnet)
    ///
    /// The friendbot URL is retrieved first from the [Options::friendbot_url] if provided
    /// or from the [Server::get_network] method. There is no friendbot faucet on mainnet.
    ///
    /// [friendbot]: https://developers.stellar.org/docs/learn/fundamentals/networks#friendbot
    pub async fn request_airdrop(&self, account_id: &str) -> Result<Account, Error> {
        let friendbot_url = if let Some(url) = self.friendbot_url.clone() {
            url
        } else {
            let network = self.get_network().await?;
            if let Some(url) = network.friendbot_url {
                url
            } else {
                return Err(Error::NoFriendbot);
            }
        };

        let client = reqwest::ClientBuilder::new()
            .build()
            .map_err(Error::NetworkError)?;

        let response = client
            .get(friendbot_url + "?addr=" + account_id)
            .send()
            .map_err(Error::NetworkError)
            .await?;

        let data: friendbot::FriendbotResponse =
            response.json().map_err(Error::NetworkError).await?;

        if let Some(success) = data.successful {
            if success {
                self.get_account(account_id).await
            } else {
                Err(Error::AccountNotFound)
            }
        } else {
            // If we don't get a success, it can be already funded
            self.get_account(account_id).await
        }
    }
}

fn handle_response<T>(response: Response<T>) -> Result<T, Error> {
    if let Some(result) = response.result {
        Ok(result)
    } else if let Some(error) = response.error {
        Err(Error::RPCError {
            code: error.code,
            message: error.message.unwrap_or_default(),
        })
    } else {
        Err(Error::UnexpectedError)
    }
}

#[cfg(test)]
mod test {}
