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
use stellar_baselib::hashing::HashingBehavior;
use stellar_baselib::hashing::Sha256Hasher;
use stellar_baselib::keypair::KeypairBehavior;
use stellar_baselib::transaction::{Transaction, TransactionBehavior};
use stellar_baselib::transaction_builder::TransactionBuilderBehavior;
use stellar_baselib::xdr::{
    ContractDataDurability, Hash, LedgerEntryData, LedgerKey, LedgerKeyAccount,
    LedgerKeyContractData, Limits, ScAddress, ScVal, WriteXdr,
};
pub const SUBMIT_TRANSACTION_TIMEOUT: u32 = 60 * 1000;

#[derive(Debug, PartialEq, Eq)]
pub enum Durability {
    Temporary,
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
    /// # use soroban_client::{Server, Options};
    /// # use soroban_client::error::Error;
    /// # async fn events() -> Result<(), Error> {
    /// # let server = Server::new("https://rpc.server", Options::default())?;
    /// let events = server.get_events(
    ///     EventLedger::From(67000),
    ///     vec![
    ///         EventFilter::new(EventType::All).contract("CAA...")
    ///     ],
    ///     Some(12)
    /// ).await?;
    /// # return Ok(()); }
    ///
    /// ```
    ///
    /// [getEvents]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getEvents
    ///
    pub async fn get_events(
        &self,
        ledger: EventLedger,
        filters: Vec<EventFilter>,
        limit: Option<u32>,
    ) -> Result<GetEventsResponse, Error> {
        let (start_ledger, end_ledger, cursor) = match ledger {
            EventLedger::From(s) => (Some(s), None, None),
            EventLedger::FromTo(s, e) => (Some(s), Some(e), None),
            EventLedger::Cursor(c) => (None, None, Some(c)),
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
                "limit": limit
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

    // TODO get_ledgers

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

    // # Call to RPC method [getTransaction]
    //
    // The getTransaction method provides details about the specified transaction.
    //
    // Clients are expected to periodically query this method to ascertain when a transaction has
    // been successfully recorded on the blockchain. The stellar-rpc system maintains a restricted
    // history of recently processed transactions, with the default retention window set at 24
    // hours.
    //
    // For private soroban-rpc instances, it is possible to modify the retention window
    // value by adjusting the transaction-retention-window configuration setting, but we do not
    // recommend values longer than 7 days. For debugging needs that extend beyond this timeframe,
    // it is advisable to index this data yourself, employ a third-party indexer, or query Hubble
    // (our public BigQuery data set).
    //
    // [getTransaction]: https://developers.stellar.org/docs/data/rpc/api-reference/methods/getTransaction
    //
    pub async fn get_transaction(&self, hash: &str) -> Result<GetTransactionResponse, Error> {
        let params = json!({
                "hash": hash
        });

        let response = self.client.post("getTransaction", params).await?;
        handle_response(response)
    }

    // TODO get_transactions

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
        let hex_contract_val = &hex::encode(Sha256Hasher::hash(contract.as_bytes()));
        let hex_id = hex_contract_val.as_bytes();
        let mut array = [0u8; 32];
        array.copy_from_slice(&hex_id[0..32]);

        let sc_address =
            ScAddress::Contract(Hash::from_str(hex_contract_val).map_err(|_| Error::XdrError)?);

        let contract_key = LedgerKey::ContractData(LedgerKeyContractData {
            key: key.clone(),
            contract: sc_address.clone(),
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
        network_passphrase: &str,
    ) -> Result<Transaction, Error> {
        let sim_response = self.simulate_transaction(transaction.clone(), None).await?;

        Ok(assemble_transaction(transaction, network_passphrase, sim_response)?.build())
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
