use core::panic;
use http::Uri;
use serde_json::json;
use stellar_baselib::hashing::Sha256Hasher;
use std::option::Option;
use std::{collections::HashMap, str::FromStr};
use stellar_baselib::account::Account;
use stellar_baselib::transaction::{Transaction, TransactionBehavior};
use stellar_xdr::next::{
    ContractDataDurability, Hash, LedgerKeyContractData, ScAddress, ScVal,
    TransactionEnvelope, TransactionMeta, TransactionResult, Limits,
};
use stellar_baselib::hashing::HashingBehavior;
use stellar_xdr::next::{LedgerEntryData, LedgerKey, LedgerKeyAccount, ReadXdr, WriteXdr};
use crate::http_client::create_client;
use crate::soroban_rpc::soroban_rpc::{
    self, GetAnyTransactionResponse, GetHealthResponse, GetLatestLedgerResponse,
    GetNetworkResponse, GetSuccessfulTransactionResponse, GetTransactionResponse,
    GetTransactionStatus, LedgerEntryResult, RawGetTransactionResponse,
    RawSimulateTransactionResponse, SendTransactionResponse, SimulateTransactionResponse, RawLedgerEntryResult, GetHealtWrapperResponse, GetNetworkResponseWrapper, RawGetTransactionResponseWrapper, GetLedgerEntriesResponseWrapper,
};
use stellar_baselib::account::AccountBehavior;
use crate::transaction::SimulationResponse::Normal;
use crate::transaction::{assemble_transaction, parse_raw_simulation, Either};
use crate::{jsonrpc::post, soroban_rpc::soroban_rpc::EventFilter};
use std::error::Error;
use stellar_baselib::keypair::KeypairBehavior;
use stellar_baselib::transaction_builder::TransactionBuilderBehavior;
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
    pub allow_http: Option<bool>,
    pub timeout: Option<u32>,
    pub headers: Option<HashMap<String, String>>,
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
            createdAt: None,
            applicationOrder: None,
            feeBump: None,
            envelopeXdr: None,
            resultXdr: None,
            resultMetaXdr: None,
            returnValue: None,
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
    ) -> Result<soroban_rpc::GetLedgerEntriesResponseWrapper, reqwest::Error> {
        let mut data: Vec<(LedgerKey, serde_json::Value)> = vec![];

        for i in 0..keys.len() {
            data.push((
                keys[i].clone(),
                serde_json::Value::String(keys[i].clone().to_xdr_base64(Limits::none()).unwrap()),
            ))
        }

        let map: std::collections::HashMap<String, serde_json::Value> = data
            .into_iter()
            .map(|(key, value)| (key.to_xdr_base64(Limits::none()).unwrap(), value))
            .collect();

        let keys: Vec<String> = map.keys().cloned().collect();

        println!("{:?}",keys);

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "getLedgerEntries",
            "params": {
                "keys": keys
            }
        });
        
        self.client
            .post(&format!("{}", &self.server_url))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?
            .json::<GetLedgerEntriesResponseWrapper>()
            .await
        
    }

    pub async fn get_account(&self, address: &str) -> Result<Account, Box<dyn Error>> {
        
        let ledger_key = LedgerKey::Account(LedgerKeyAccount {
            account_id: stellar_baselib::keypair::Keypair::from_public_key(address)
                .unwrap()
                .xdr_account_id(),
        });

        let resp = self.get_ledger_entries(vec![ledger_key]).await?;


        let entries = match resp.result.entries {
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
        let account_entry = match LedgerEntryData::from_xdr_base64(ledger_entry_data, Limits::none()).unwrap() {
            LedgerEntryData::Account(x) => x,
            _ => panic!("Invalid"),
        };

        Ok(Account::new(address, &account_entry.seq_num.0.to_string()).unwrap())
    }

    pub async fn get_health(&self) -> Result<GetHealtWrapperResponse, reqwest::Error> {
        
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "getHealth"
        });
        
        self.client
            .post(&format!("{}", &self.server_url))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?
            .json::<GetHealtWrapperResponse>()
            .await
    }

    pub async fn get_network(&self) -> Result<GetNetworkResponseWrapper, reqwest::Error> {

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 8675309,
            "method": "getNetwork"
        });

        let val = self.client
            .post(&format!("{}", &self.server_url))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?
            .json::<GetNetworkResponseWrapper>()
            .await;
        // println!("{:?}", val);
        
        val
    
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
            transaction.to_envelope().unwrap().to_xdr_base64(Limits::none()).unwrap(),
            serde_json::Value::String(transaction.to_envelope().unwrap().to_xdr_base64(Limits::none()).unwrap()),
        ));

        let map: std::collections::HashMap<String, serde_json::Value> =
            data.into_iter().map(|(key, value)| (key, value)).collect();

        println!("Hunter");
        
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
        let hex_contract_val = &hex::encode(Sha256Hasher::hash(contract.as_bytes()));
        println!("Hex {}", hex_contract_val);
        let hex_id = hex_contract_val.as_bytes();
        let mut array = [0u8; 32];
        array.copy_from_slice(&hex_id[0..32]);

        let sc_address = ScAddress::Contract(Hash::from_str(hex_contract_val).unwrap());

        let contract_key = LedgerKey::ContractData(LedgerKeyContractData {
            key: key.clone(),
            contract: sc_address.clone(),
            durability: durability.to_xdr(),
            // body_type: ContractEntryBodyType::DataEntry,
        });

        let val = vec![contract_key];

        let response = self.get_ledger_entries(val).await?;

        println!(" Response {:?}",response);
        
        match response.result.entries.unwrap().get(0) {
            Some(entry) => Ok(entry.clone()),
            None => Err(format!(
                "Contract data not found. Contract: {}, Key: {:?}, Durability: {:?}",
                sc_address.clone().to_xdr_base64(Limits::none()).unwrap(),
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
        
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 8675309,
            "method": "getTransaction",
            "params": {
                "hash": hash
            }
        });

        let raw = self.client
            .post(&format!("{}", &self.server_url))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?
            .json::<RawGetTransactionResponseWrapper>()
            .await?;

        let mut success_info = GetSuccessfulTransactionResponse {
            base: None,
            ledger: None,
            createdAt: None,
            applicationOrder: None,
            feeBump: None,
            envelopeXdr: None,
            resultXdr: None,
            resultMetaXdr: None,
            returnValue: None,
        };

        if let GetTransactionStatus::SUCCESS = raw.result.status {
            let meta = TransactionMeta::from_xdr_base64(&raw.result.resultMetaXdr.unwrap(), Limits::none()).unwrap();

            success_info.ledger = raw.result.ledger;
            success_info.createdAt = Some(i32::from_str(&raw.result.createdAt.unwrap()).unwrap());
            success_info.applicationOrder = raw.result.applicationOrder;
            success_info.feeBump = raw.result.feeBump;
            success_info.envelopeXdr =
                TransactionEnvelope::from_xdr_base64(&raw.result.envelopeXdr.unwrap(), Limits::none()).ok();
            success_info.resultXdr =
                TransactionResult::from_xdr_base64(&raw.result.resultXdr.unwrap(), Limits::none()).ok();
            success_info.resultMetaXdr = Some(meta.clone());

            let f = GetAnyTransactionResponse {
                status: raw.result.status,
                latestLedger: raw.result.latestLedger,
                latestLedgerCloseTime: i32::from_str(&raw.result.latestLedgerCloseTime).unwrap(),
                oldestLedger: raw.result.oldestLedger,
                oldestLedgerCloseTime: i32::from_str(&raw.result.oldestLedgerCloseTime).unwrap(),
            };

            success_info.base = Some(f);

            if let TransactionMeta::V3(v3) = meta {
                if let Some(soroban_meta) = v3.soroban_meta {
                    success_info.returnValue = Some(soroban_meta.return_value);
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

        println!("{:?}", sim_response);
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
            transaction.to_envelope().unwrap().to_xdr_base64(Limits::none()).unwrap(),
            serde_json::Value::String(transaction.to_envelope().unwrap().to_xdr_base64(Limits::none()).unwrap()),
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
    #[allow(unused)]
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
