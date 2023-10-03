use std::{collections::HashMap, str::FromStr};
use std::option::Option;
use http::Uri;
use stellar_baselib::transaction::Transaction;
use stellar_baselib::{op_list::create_account::create_account, account::Account};
use stellar_xdr::{next::{LedgerKey, LedgerKeyAccount, LedgerEntryData, ReadXdr, WriteXdr}, curr::{PublicKey}};
use serde::{Serialize, Deserialize};
use std::error::Error;

use crate::http_client::create_client;
use crate::soroban_rpc::soroban_rpc::{self, GetHealthResponse, GetNetworkResponse, GetLatestLedgerResponse, SimulateTransactionResponse, RawSimulateTransactionResponse};
use crate::transaction::{parse_raw_simulation, Either};
use crate::{soroban_rpc::soroban_rpc::EventFilter, jsonrpc::post};

// Assuming you'll need to convert other parts of your TypeScript program,
// you might need libraries like `reqwest` for making HTTP requests and `serde` for serialization/deserialization.

const SUBMIT_TRANSACTION_TIMEOUT: u32 = 60 * 1000;

#[derive(Debug, PartialEq, Eq)]
pub enum Durability {
    Temporary,
    Persistent,
}

pub struct GetEventsRequest {
    filters: Vec<EventFilter>, // placeholder for actual type
    start_ledger: Option<u32>,
    cursor: Option<String>,
    limit: Option<u32>,
}

pub struct Options {
    allow_http: Option<bool>,
    timeout: Option<u32>,
    headers: Option<HashMap<String, String>>,
}

pub struct Server {
    server_url: Uri,
    client: reqwest::Client
}

impl Server {
    pub fn new(server_url: &str, opts: Options) -> Self {
        let server_url = Uri::from_str(server_url).unwrap();

        if !server_url.scheme().unwrap().as_str().starts_with("https") && !opts.allow_http.unwrap_or(false) {
            panic!("Cannot connect to insecure Soroban RPC server if `allow_http` isn't set"); // or return a Result with an error
        }

        Server { server_url, client: create_client() }
    }

    pub async fn get_ledger_entries(&self, keys: Vec<LedgerKey>) -> Result<soroban_rpc::GetLedgerEntriesResponse, reqwest::Error> {
        
        let mut data: Vec<(LedgerKey, serde_json::Value)> = vec![];

        for i in 0..keys.len() {
            data.push((keys[i].clone(), serde_json::Value::String(keys[i].clone().to_xdr_base64().unwrap())))
        }

        let map: std::collections::HashMap<String, serde_json::Value> = data.into_iter()
        .map(|(key, value)| (key.to_xdr_base64().unwrap(), value))
        .collect();

        let dd = self.server_url.clone().to_string();

        let val = post::<soroban_rpc::GetLedgerEntriesResponse>(
            &dd,
            "getLedgerEntries",
            map,
        );

        val.await
    }

    pub async fn get_account(&self, address: &str) -> Result<Account, Box<dyn Error>> {
        let ledger_key = LedgerKey::Account(
            LedgerKeyAccount {
                account_id: stellar_baselib::keypair::Keypair::from_public_key(address).unwrap().xdr_account_id(),
            }
        );
        
        let resp = self.get_ledger_entries(vec![ledger_key]).await?;
        let entries = match resp.entries {
            Some(e) => e,
            None => Vec::new(),
        };

        if entries.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Account not found: {}", address),
            )));
        }

        let ledger_entry_data = &entries[0].xdr;
        let account_entry = match LedgerEntryData::from_xdr_base64(ledger_entry_data).unwrap() {
            LedgerEntryData::Account(x) => x,
            _ => panic!("Invalid")
        };

       Ok(Account::new(address, &account_entry.seq_num.0.to_string()).unwrap())
    }

    pub async fn get_health(&self) -> Result<GetHealthResponse, reqwest::Error> {
        self.client.get(&format!("{}/getHealth", &self.server_url))
            .send()
            .await?
            .json::<GetHealthResponse>()
            .await
    }

    pub async fn get_network(&self) -> Result<GetNetworkResponse, reqwest::Error> {
        post::<soroban_rpc::GetNetworkResponse>(&self.server_url.to_string(), "getNetwork", HashMap::new()).await
    }

    pub async fn get_latest_ledger(&self) -> Result<GetLatestLedgerResponse, reqwest::Error> {
        post::<soroban_rpc::GetLatestLedgerResponse>(&self.server_url.to_string(), "getLatestLedger", HashMap::new()).await
    }
    
    pub async fn simulate_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<SimulateTransactionResponse, reqwest::Error> {

        let mut data: Vec<(String, serde_json::Value)> = vec![];

        data.push((transaction.to_envelope().unwrap().to_xdr_base64().unwrap(), serde_json::Value::String(transaction.to_envelope().unwrap().to_xdr_base64().unwrap())));

        let map: std::collections::HashMap<String, serde_json::Value> = data.into_iter()
        .map(|(key, value)| (key, value))
        .collect();


        let raw_response = Either::Right(
            post::<RawSimulateTransactionResponse>(
                &self.server_url.to_string(),
                "simulateTransaction",
                map
            ).await?
        );
            

        Ok(parse_raw_simulation(raw_response))
    }
}

