use core::panic;
use http::Uri;
use std::option::Option;
use std::{collections::HashMap, str::FromStr};
use stellar_baselib::account::Account;
use stellar_baselib::transaction::Transaction;
use stellar_xdr::next::{
    ContractDataDurability, Hash, LedgerKeyContractData, ScAddress, ScVal,
    TransactionEnvelope, TransactionMeta, TransactionResult,
};
use stellar_xdr::next::{LedgerEntryData, LedgerKey, LedgerKeyAccount, ReadXdr, WriteXdr};

use crate::http_client::create_client;
use crate::soroban_rpc::soroban_rpc::{
    self, GetAnyTransactionResponse, GetHealthResponse, GetLatestLedgerResponse,
    GetNetworkResponse, GetSuccessfulTransactionResponse, GetTransactionResponse,
    GetTransactionStatus, LedgerEntryResult, RawGetTransactionResponse,
    RawSimulateTransactionResponse, SendTransactionResponse, SimulateTransactionResponse,
};
use crate::transaction::SimulationResponse::Normal;
use crate::transaction::{assemble_transaction, parse_raw_simulation, Either};
use crate::{jsonrpc::post, soroban_rpc::soroban_rpc::EventFilter};
use std::error::Error;

// Assuming you'll need to convert other parts of your TypeScript program,
// you might need libraries like `reqwest` for making HTTP requests and `serde` for serialization/deserialization.

const SUBMIT_TRANSACTION_TIMEOUT: u32 = 60 * 1000;

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
    client: reqwest::Client,
}

impl Default for GetSuccessfulTransactionResponse {
    fn default() -> Self {
        Self {
            base: None,
            ledger: None,
            created_at: None,
            application_order: None,
            fee_bump: None,
            envelope_xdr: None,
            result_xdr: None,
            result_meta_xdr: None,
            return_value: None,
        }
    }
}

impl Server {
    pub fn new(server_url: &str, opts: Options) -> Self {
        let server_url = Uri::from_str(server_url).unwrap();

        if !server_url.scheme().unwrap().as_str().starts_with("https")
            && !opts.allow_http.unwrap_or(false)
        {
            panic!("Cannot connect to insecure Soroban RPC server if `allow_http` isn't set");
            // or return a Result with an error
        }

        Server {
            server_url,
            client: create_client(),
        }
    }

    pub async fn get_ledger_entries(
        &self,
        keys: Vec<LedgerKey>,
    ) -> Result<soroban_rpc::GetLedgerEntriesResponse, reqwest::Error> {
        let mut data: Vec<(LedgerKey, serde_json::Value)> = vec![];

        for i in 0..keys.len() {
            data.push((
                keys[i].clone(),
                serde_json::Value::String(keys[i].clone().to_xdr_base64().unwrap()),
            ))
        }

        let map: std::collections::HashMap<String, serde_json::Value> = data
            .into_iter()
            .map(|(key, value)| (key.to_xdr_base64().unwrap(), value))
            .collect();

        let dd = self.server_url.clone().to_string();

        let val = post::<soroban_rpc::GetLedgerEntriesResponse>(&dd, "getLedgerEntries", map);

        val.await
    }

    pub async fn get_account(&self, address: &str) -> Result<Account, Box<dyn Error>> {
        let ledger_key = LedgerKey::Account(LedgerKeyAccount {
            account_id: stellar_baselib::keypair::Keypair::from_public_key(address)
                .unwrap()
                .xdr_account_id(),
        });

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
            _ => panic!("Invalid"),
        };

        Ok(Account::new(address, &account_entry.seq_num.0.to_string()).unwrap())
    }

    pub async fn get_health(&self) -> Result<GetHealthResponse, reqwest::Error> {
        self.client
            .get(&format!("{}/getHealth", &self.server_url))
            .send()
            .await?
            .json::<GetHealthResponse>()
            .await
    }

    pub async fn get_network(&self) -> Result<GetNetworkResponse, reqwest::Error> {
        post::<soroban_rpc::GetNetworkResponse>(
            &self.server_url.to_string(),
            "getNetwork",
            HashMap::new(),
        )
        .await
    }

    pub async fn get_latest_ledger(&self) -> Result<GetLatestLedgerResponse, reqwest::Error> {
        post::<soroban_rpc::GetLatestLedgerResponse>(
            &self.server_url.to_string(),
            "getLatestLedger",
            HashMap::new(),
        )
        .await
    }

    pub async fn simulate_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<SimulateTransactionResponse, reqwest::Error> {
        let mut data: Vec<(String, serde_json::Value)> = vec![];

        data.push((
            transaction.to_envelope().unwrap().to_xdr_base64().unwrap(),
            serde_json::Value::String(transaction.to_envelope().unwrap().to_xdr_base64().unwrap()),
        ));

        let map: std::collections::HashMap<String, serde_json::Value> =
            data.into_iter().map(|(key, value)| (key, value)).collect();

        let raw_response = Either::Right(
            post::<RawSimulateTransactionResponse>(
                &self.server_url.to_string(),
                "simulateTransaction",
                map,
            )
            .await?,
        );

        Ok(parse_raw_simulation(raw_response))
    }

    pub async fn get_contract_data(
        &self,
        contract: &str,
        key: ScVal,
        durability: Durability,
    ) -> Result<LedgerEntryResult, Box<dyn std::error::Error>> {
        let sc_address = ScAddress::Contract(Hash::from_str(contract).unwrap());

        let contract_key = LedgerKey::ContractData(LedgerKeyContractData {
            key: key.clone(),
            contract: sc_address.clone(),
            durability: durability.to_xdr(),
            // body_type: ContractEntryBodyType::DataEntry,
        });

        let val = vec![contract_key];

        let response = self.get_ledger_entries(val).await?;

        match response.entries.unwrap().get(0) {
            Some(entry) => Ok(entry.clone()),
            None => Err(format!(
                "Contract data not found. Contract: {}, Key: {:?}, Durability: {:?}",
                sc_address.clone().to_xdr_base64().unwrap(),
                key,
                durability
            )
            .into()),
        }
    }

    pub async fn get_transaction(
        &self,
        hash: &str,
    ) -> Result<GetTransactionResponse, reqwest::Error> {
        let mut data: Vec<(String, serde_json::Value)> = vec![];

        data.push((
            hash.to_string(),
            serde_json::Value::String(hash.to_string()),
        ));

        let map: std::collections::HashMap<String, serde_json::Value> =
            data.into_iter().map(|(key, value)| (key, value)).collect();

        let raw =
            post::<RawGetTransactionResponse>(&self.server_url.to_string(), "getTransaction", map)
                .await?;

        let mut success_info = GetSuccessfulTransactionResponse {
            base: None,
            ledger: None,
            created_at: None,
            application_order: None,
            fee_bump: None,
            envelope_xdr: None,
            result_xdr: None,
            result_meta_xdr: None,
            return_value: None,
        };

        if let GetTransactionStatus::SUCCESS = raw.status {
            let meta = TransactionMeta::from_xdr_base64(&raw.result_meta_xdr.unwrap()).unwrap();

            success_info.ledger = raw.ledger;
            success_info.created_at = raw.created_at;
            success_info.application_order = raw.application_order;
            success_info.fee_bump = raw.fee_bump;
            success_info.envelope_xdr =
                TransactionEnvelope::from_xdr_base64(&raw.envelope_xdr.unwrap()).ok();
            success_info.result_xdr =
                TransactionResult::from_xdr_base64(&raw.result_xdr.unwrap()).ok();
            success_info.result_meta_xdr = Some(meta.clone());

            let f = GetAnyTransactionResponse {
                status: raw.status,
                latest_ledger: raw.latest_ledger,
                latest_ledger_close_time: raw.latest_ledger,
                oldest_ledger: raw.oldest_ledger,
                oldest_ledger_close_time: raw.oldest_ledger_close_time,
            };

            success_info.base = Some(f);

            if let TransactionMeta::V3(v3) = meta {
                if let Some(soroban_meta) = v3.soroban_meta {
                    success_info.return_value = Some(soroban_meta.return_value);
                }
            }
        }

        let val = GetTransactionResponse::Successful(success_info);

        Ok(val)
    }

    pub async fn prepare_transaction(
        &self,
        transaction: Transaction,
        network_passphrase: Option<&str>,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
        let sim_response = self
            .simulate_transaction(transaction.clone())
            .await
            .unwrap();

        //TODO: Error Handling

        Ok(assemble_transaction(
            transaction,
            &network_passphrase.unwrap(),
            Normal(sim_response),
        )
        .unwrap()
        .build())
    }

    pub async fn send_transaction(
        &self,
        transaction: Transaction,
        // Assuming you have an enum or similar to differentiate Transaction from FeeBumpTransaction
    ) -> Result<SendTransactionResponse, Box<dyn std::error::Error>> {
        let mut data: Vec<(String, serde_json::Value)> = vec![];

        data.push((
            transaction.to_envelope().unwrap().to_xdr_base64().unwrap(),
            serde_json::Value::String(transaction.to_envelope().unwrap().to_xdr_base64().unwrap()),
        ));

        let map: std::collections::HashMap<String, serde_json::Value> =
            data.into_iter().map(|(key, value)| (key, value)).collect();

        // Assuming `jsonrpc` and `SendTransactionResponse` types are defined somewhere
        let response =
            post::<SendTransactionResponse>(&self.server_url.to_string(), "sendTransaction", map)
                .await;

        Ok(response?)
    }
    //TODO: getEvents
    //TODO: request airdrop
    fn find_created_account_sequence_in_transaction_meta(
        meta: TransactionMeta,
    ) -> Result<String, &'static str> {
        let operations = match meta {
            TransactionMeta::V0(ops) => ops,
            TransactionMeta::V1(v3_meta) => v3_meta.operations,
            TransactionMeta::V2(v3_meta) => v3_meta.operations,
            TransactionMeta::V3(v3_meta) => v3_meta.operations,
        };

        let operations = operations.to_vec();

        for op in operations {
            for change in op.changes.0.to_vec() {
                match change {
                    stellar_xdr::next::LedgerEntryChange::Created(x) => match x.data {
                        LedgerEntryData::Account(ae) => return Ok(ae.seq_num.0.to_string()),
                        _ => (),
                    },
                    _ => (),
                }
            }
        }

        Err("No account created in transaction")
    }
}
