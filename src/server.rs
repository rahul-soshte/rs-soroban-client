use crate::error::*;
use crate::http_client::create_client;
use crate::jsonrpc::post;
use crate::soroban_rpc::*;
use crate::transaction::assemble_transaction;
use crate::transaction::SimulationResponse::Raw;
use futures::TryFutureExt;
use http::Uri;
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
    ContractDataDurability, DiagnosticEvent, Hash, LedgerEntryChange, LedgerEntryData, LedgerKey,
    LedgerKeyAccount, LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal,
    TransactionEnvelope, TransactionMeta, TransactionResult, WriteXdr,
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

#[derive(Debug)]
pub struct Server {
    server_url: Uri,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLeeway {
    #[serde(rename = "cpuInstructions")]
    pub cpu_instructions: u64,
}

impl Server {
    pub fn new(server_url: &str, opts: Options) -> Result<Self, Error> {
        let server_url = Uri::from_str(server_url)
            .map_err(|e| Error::InvalidRpc(InvalidRpcUrl::InvalidUri(e)))?;

        let allow_http = opts.allow_http.unwrap_or(false);
        match server_url.scheme() {
            Some(s) if s.as_str() == "http" => {
                if !allow_http {
                    return Err(Error::InvalidRpc(InvalidRpcUrl::UnsecureHttpNotAllowed));
                }
            }
            Some(s) if s.as_str() == "https" => {
                // all good
            }
            _ => {
                return Err(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme));
            }
        }

        Ok(Server {
            server_url,
            client: create_client(),
        })
    }

    pub async fn get_ledger_entries(
        &self,
        keys: Vec<LedgerKey>,
    ) -> Result<GetLedgerEntriesResponseWrapper, Error> {
        let keys: Result<Vec<String>, Error> = keys
            .into_iter()
            .map(|k| k.to_xdr_base64(Limits::none()).map_err(|_| Error::XdrError))
            .collect();

        match keys {
            Ok(keys) => {
                let payload = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "getLedgerEntries",
                    "params": {
                        "keys": keys
                    }
                });

                self.client
                    .post(self.server_url.to_string())
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|_| Error::NetworkError)?
                    .json::<GetLedgerEntriesResponseWrapper>()
                    .await
                    .map_err(|_| Error::NetworkError)
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
        let entries = resp.result.entries.unwrap_or_default();
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

    pub async fn get_health(&self) -> Result<GetHealtWrapperResponse, Error> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth"
        });

        self.client
            .post(self.server_url.to_string())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|_| Error::NetworkError)
            .await?
            .json::<GetHealtWrapperResponse>()
            .map_err(|_| Error::NetworkError)
            .await
    }

    pub async fn get_network(&self) -> Result<GetNetworkResponseWrapper, Error> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getNetwork"
        });

        self.client
            .post(self.server_url.to_string())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|_| Error::NetworkError)
            .await?
            .json::<GetNetworkResponseWrapper>()
            .map_err(|_| Error::NetworkError)
            .await
    }

    pub async fn get_latest_ledger(&self) -> Result<GetLatestLedgerResponse, Error> {
        post::<GetLatestLedgerResponse>(
            &self.server_url.to_string(),
            "getLatestLedger",
            HashMap::new(),
        )
        .map_err(|_| Error::NetworkError)
        .await
    }

    pub async fn simulate_transaction(
        &self,
        transaction: Transaction,
        addl_resources: Option<ResourceLeeway>,
    ) -> Result<RawSimulateTransactionResponse, Error> {
        let transaction_xdr = transaction
            .to_envelope()
            .map_err(|_| Error::TransactionError)?
            .to_xdr_base64(Limits::none())
            .map_err(|_| Error::XdrError)?;

        let mut params = json!({
            "transaction": transaction_xdr
        });

        // Add resource config if provided
        if let Some(resources) = addl_resources {
            params = json!({
                "transaction": transaction_xdr,
                "resourceConfig": {
                    "instructionLeeway": resources.cpu_instructions
                }
            });
        }

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": params
        });

        let response = self
            .client
            .post(self.server_url.to_string())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|_| Error::NetworkError)
            .await?;

        let result: JsonRpcSimulateResponse =
            response.json().map_err(|_| Error::NetworkError).await?;
        Ok(result.result)
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

        if let Some(entries) = response.result.entries {
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
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": {
                "hash": hash
            }
        });

        let raw = self
            .client
            .post(self.server_url.to_string())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|_| Error::NetworkError)
            .await?
            .json::<RawGetTransactionResponseWrapper>()
            .map_err(|_| Error::NetworkError)
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
            // Set the basic fields that don't depend on XDR parsing
            success_info.ledger = raw.result.ledger;
            success_info.applicationOrder = raw.result.applicationOrder;
            success_info.feeBump = raw.result.feeBump;

            // Handle optional createdAt
            if let Some(created_at) = raw.result.createdAt {
                success_info.createdAt = Some(i32::from_str(&created_at).unwrap_or(-1));
            }

            // Handle optional envelopeXdr
            if let Some(envelope_xdr) = &raw.result.envelopeXdr {
                success_info.envelopeXdr =
                    TransactionEnvelope::from_xdr_base64(envelope_xdr, Limits::none()).ok();
            }

            // Handle optional resultXdr
            if let Some(result_xdr) = &raw.result.resultXdr {
                success_info.resultXdr =
                    TransactionResult::from_xdr_base64(result_xdr, Limits::none()).ok();
            }

            // Handle optional resultMetaXdr
            if let Some(meta_xdr) = &raw.result.resultMetaXdr {
                if let Ok(meta) = TransactionMeta::from_xdr_base64(meta_xdr, Limits::none()) {
                    success_info.resultMetaXdr = Some(meta.clone());

                    // Extract return value if it's a V3 transaction with Soroban metadata
                    if let TransactionMeta::V3(v3) = meta {
                        if let Some(soroban_meta) = v3.soroban_meta {
                            success_info.returnValue = Some(soroban_meta.return_value.clone());
                        }
                    }
                }
            }

            // Set the base response info
            let base = GetAnyTransactionResponse {
                status: raw.result.status,
                latestLedger: raw.result.latestLedger,
                latestLedgerCloseTime: i32::from_str(&raw.result.latestLedgerCloseTime)
                    .unwrap_or(-1),
                oldestLedger: raw.result.oldestLedger,
                oldestLedgerCloseTime: i32::from_str(&raw.result.oldestLedgerCloseTime)
                    .unwrap_or(-1),
            };
            success_info.base = Some(base);

            Ok(GetTransactionResponse::Successful(success_info))
        } else if let GetTransactionStatus::NOT_FOUND = raw.result.status {
            let base = GetAnyTransactionResponse {
                status: raw.result.status,
                latestLedger: raw.result.latestLedger,
                latestLedgerCloseTime: i32::from_str(&raw.result.latestLedgerCloseTime)
                    .unwrap_or(-1),
                oldestLedger: raw.result.oldestLedger,
                oldestLedgerCloseTime: i32::from_str(&raw.result.oldestLedgerCloseTime)
                    .unwrap_or(-1),
            };
            Ok(GetTransactionResponse::Missing(
                GetMissingTransactionResponse { base },
            ))
        } else {
            let base = GetAnyTransactionResponse {
                status: raw.result.status,
                latestLedger: raw.result.latestLedger,
                latestLedgerCloseTime: i32::from_str(&raw.result.latestLedgerCloseTime)
                    .unwrap_or(-1),
                oldestLedger: raw.result.oldestLedger,
                oldestLedgerCloseTime: i32::from_str(&raw.result.oldestLedgerCloseTime)
                    .unwrap_or(-1),
            };
            Ok(GetTransactionResponse::Failed(
                GetFailedTransactionResponse { base },
            ))
        }
    }

    pub async fn prepare_transaction(
        &self,
        transaction: Transaction,
        network_passphrase: &str,
    ) -> Result<Transaction, Error> {
        let sim_response = self.simulate_transaction(transaction.clone(), None).await?;

        Ok(assemble_transaction(transaction, network_passphrase, Raw(sim_response))?.build())
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

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": {
                "transaction": transaction_xdr
            }
        });

        let response = self
            .client
            .post(self.server_url.to_string())
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|_| Error::NetworkError)
            .await?;

        let result: JsonRpcResponse = response.json().map_err(|_| Error::NetworkError).await?;

        /*Not sure why this is here
                // If error result is present, decode it
                if let Some(error_xdr) = &result.result.error_result {
                    println!(
                        "Transaction error: {:?}",
                        TransactionResult::from_xdr_base64(error_xdr, Limits::none())
                            .expect("Xdr from RPC should be valid")
                    );
                }

                // If diagnostic events are present, decode them
                if let Some(events) = &result.result.diagnostic_events {
                    for event_xdr in events {
                        println!(
                            "Diagnostic event: {:?}",
                            DiagnosticEvent::from_xdr_base64(event_xdr, Limits::none())
                                .expect("Xdr from RPC should be valid")
                        );
                    }
                }
        */

        Ok(result.result)
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
                if let LedgerEntryChange::Created(x) = change {
                    if let LedgerEntryData::Account(ae) = x.data {
                        return Ok(ae.seq_num.0.to_string());
                    }
                }
            }
        }

        Err("No account created in transaction")
    }
}

#[cfg(test)]
mod test {

    use crate::server::{Error, InvalidRpcUrl};

    use super::Server;

    #[test]
    fn server_new() {
        let s1 = Server::new(
            "https://rpc",
            super::Options {
                allow_http: None,
                timeout: None,
                headers: None,
            },
        );
        assert!(s1.is_ok(), "https scheme with allow_http None");

        let s2 = Server::new(
            "/rpc",
            super::Options {
                allow_http: None,
                timeout: None,
                headers: None,
            },
        );
        assert!(matches!(
            s2.err(),
            Some(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme)),
        ));

        let s3 = Server::new(
            "/rpc",
            super::Options {
                allow_http: Some(true),
                timeout: None,
                headers: None,
            },
        );
        assert!(matches!(
            s3.err(),
            Some(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme)),
        ));

        let s4 = Server::new(
            "http://rpc",
            super::Options {
                allow_http: Some(true),
                timeout: None,
                headers: None,
            },
        );
        assert!(s4.is_ok(), "http scheme with allow_http true");

        let s5 = Server::new(
            "",
            super::Options {
                allow_http: Some(true),
                timeout: None,
                headers: None,
            },
        );
        assert!(matches!(
            s5.err(),
            Some(Error::InvalidRpc(InvalidRpcUrl::InvalidUri(_))),
        ));

        let s6 = Server::new(
            "http://rpc",
            super::Options {
                allow_http: Some(false),
                timeout: None,
                headers: None,
            },
        );
        assert!(matches!(
            s6.err(),
            Some(Error::InvalidRpc(InvalidRpcUrl::UnsecureHttpNotAllowed)),
        ));
    }
}
