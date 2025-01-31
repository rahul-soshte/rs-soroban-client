use crate::error::*;
use crate::jsonrpc::{JsonRpc, Response};
use crate::soroban_rpc::*;
use crate::transaction::assemble_transaction;
use serde::{Deserialize, Serialize};
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
    LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal, WriteXdr,
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

pub struct Options {
    pub allow_http: Option<bool>,
    pub timeout: Option<u64>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug)]
pub struct Server {
    client: JsonRpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLeeway {
    #[serde(rename = "cpuInstructions")]
    pub cpu_instructions: u64,
}

impl Server {
    pub fn new(server_url: &str, opts: Options) -> Result<Self, Error> {
        let server_url = reqwest::Url::from_str(server_url)
            .map_err(|_e| Error::InvalidRpc(InvalidRpcUrl::InvalidUri))?;
        let allow_http = opts.allow_http.unwrap_or(false);
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
            client: JsonRpc::new(
                server_url,
                opts.timeout.unwrap_or(10),
                opts.headers.unwrap_or_default(),
            ),
        })
    }

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

        let ledger_entry_data = &entries[0].xdr;
        if let LedgerEntryData::Account(account_entry) =
            LedgerEntryData::from_xdr_base64(ledger_entry_data, Limits::none())
                .map_err(|_| Error::XdrError)?
        {
            Ok(Account::new(address, &account_entry.seq_num.0.to_string()).unwrap())
        } else {
            Err(Error::AccountNotFound)
        }
    }

    pub async fn get_health(&self) -> Result<GetHealthResponse, Error> {
        let response = self
            .client
            .post("getHealth", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    pub async fn get_network(&self) -> Result<GetNetworkResponse, Error> {
        let response = self
            .client
            .post("getNetwork", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

    pub async fn get_latest_ledger(&self) -> Result<GetLatestLedgerResponse, Error> {
        let response = self
            .client
            .post("getLatestLedger", serde_json::Value::Null)
            .await?;
        handle_response(response)
    }

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

    pub async fn get_transaction(&self, hash: &str) -> Result<GetTransactionResponse, Error> {
        let params = json!({
                "hash": hash
        });

        let response = self.client.post("getTransaction", params).await?;
        handle_response(response)
    }

    pub async fn prepare_transaction(
        &self,
        transaction: Transaction,
        network_passphrase: &str,
    ) -> Result<Transaction, Error> {
        let sim_response = self.simulate_transaction(transaction.clone(), None).await?;

        Ok(assemble_transaction(transaction, network_passphrase, sim_response)?.build())
    }

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
    //TODO: request airdrop
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
