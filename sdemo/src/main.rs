#![ allow(warnings)] 
use std::rc::Rc;
use std::cell::RefCell;
use soroban_client::network::{Networks, NetworkPassphrase};
use soroban_client::server::Durability;
use soroban_client::server::Options;
use soroban_client::contract_spec::native_to_sc_val;
use soroban_client::transaction::Account;
use soroban_client::transaction_builder::TransactionBuilder;
use soroban_client::{server::Server, keypair::Keypair};
use soroban_client::keypair::KeypairBehavior;
use soroban_client::transaction_builder::TransactionBuilderBehavior;
use soroban_client::account::AccountBehavior;
use stellar_baselib::{xdr, contract};
use stellar_xdr::next::{ScVec, WriteXdr, Limits, ScContractInstance};
use stellar_xdr::next::{HostFunction, ScSymbol, InvokeContractArgs, Hash, StringM, ScString, ScVal, ReadXdr, ScSpecType, ContractDataDurability};
use stellar_xdr::next::ScAddress;
use stellar_xdr::next::ScAddress::Contract;
use std::str::FromStr;
use stellar_baselib::transaction::TransactionBehavior;
use stellar_baselib::contract::ContractBehavior;
use soroban_client::transaction_builder::TIMEOUT_INFINITE;
use soroban_client::operation::PaymentOpts;
use soroban_client::operation::Operation;
use soroban_client::asset::Asset;
use soroban_client::asset::AssetBehavior;
use soroban_client::soroban_rpc::soroban_rpc::GetTransactionResponse;
use std::time::Duration;
use soroban_client::soroban_rpc::soroban_rpc::GetTransactionStatus;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize server connection
    let server = Server::new(
        "https://soroban-testnet.stellar.org",
        Options {
            allow_http: None,
            timeout: Some(1000),
            headers: None,
        }
    );

    // Set up source account
    let source_secret_key = "SBUZCKI2CUYAG7DYCLBDQ4BBBL2VJJ5OM6S6QUIVDQPU7CQOJDCUBUQ3";
    let source_keypair = Keypair::from_secret(source_secret_key).expect("Invalid secret key");
    let source_public_key = "GAJZS4EXLJFRF25VN345LA32KHTK553Q34SAPVXS7FDFVPUIMJ56ICVL";

    // Get account information from server
    let account_data = server.get_account(source_public_key).await?;
    let source_account = Rc::new(RefCell::new(
        Account::new(source_public_key, &account_data.sequence_number()).unwrap()
    ));

    // Contract interaction transaction
    let contract_id = "CCSE2AN2S4RLMMXJY5FRYQ4YN6UGG54LJT3HPWGGISMJI4OAUYOY6AVR";
    let contract = contract::Contracts::new(contract_id).unwrap();
    
    let mut contract_tx = TransactionBuilder::new(
        source_account.clone(),
        Networks::testnet(),
        None
    )
    .fee(1000000_u32)
    .add_operation(contract.call("increment", None))
    .set_timeout(TIMEOUT_INFINITE)?
    .build();

    contract_tx = server.prepare_transaction(contract_tx, Some(Networks::testnet())).await.unwrap();
    let before_signing = contract_tx.to_envelope().unwrap().to_xdr_base64(Limits::none());
    // println!("Before Signing {:?}", before_signing);
    
    // Sign the contract transaction
    contract_tx.sign(&[source_keypair.clone()]);
    let after_signing = contract_tx.to_envelope().unwrap().to_xdr_base64(Limits::none());
    // println!("After Signing {:?}", after_signing);

    match server.send_transaction(contract_tx).await {
        Ok(response) => {
            println!("Transaction sent successfully");
            println!("Transaction hash: {}", response.base.hash);
            
            // Start polling for transaction completion
            let hash = response.base.hash.clone();
            
            loop {
                match server.get_transaction(&hash).await {
                    Ok(GetTransactionResponse::Successful(success_info)) => {
                        // Check if we have base information
                        if let Some(base) = &success_info.base {
                            match base.status {
                                GetTransactionStatus::SUCCESS => {
                                    println!("Transaction successful!");
                                    if let Some(ledger) = success_info.ledger {
                                        println!("Confirmed in ledger: {}", ledger);
                                    }
                                    if let Some(return_value) = success_info.returnValue {
                                        println!("Return value: {:?}", return_value);
                                    }
                                    if let Some(meta) = success_info.resultMetaXdr {
                                        println!("Transaction metadata: {:?}", meta);
                                    }
                                    break;
                                }
                                GetTransactionStatus::FAILED => {
                                    if let Some(result) = success_info.resultXdr {
                                        eprintln!("Transaction failed with result: {:?}", result);
                                    } else {
                                        eprintln!("Transaction failed without result XDR");
                                    }
                                    break;
                                }
                                GetTransactionStatus::NOT_FOUND => {
                                    println!("Waiting for transaction confirmation... Latest ledger: {}", base.latestLedger);
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                    continue;
                                }
                            }
                        }
                    }
                    Ok(GetTransactionResponse::Failed(failed_info)) => {
                        eprintln!("Transaction failed. Latest ledger: {}", failed_info.base.latestLedger);
                        break;
                    }
                    Ok(GetTransactionResponse::Missing(missing_info)) => {
                        println!("Transaction not found. Latest ledger: {}", missing_info.base.latestLedger);
                        println!("Waiting for transaction confirmation...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Error getting transaction status: {}", e);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to send transaction: {}", e);
        }
    }

    Ok(())
}