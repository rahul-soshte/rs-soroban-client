use soroban_client::account::AccountBehavior;
use soroban_client::contract;
use soroban_client::contract::ContractBehavior;
use soroban_client::keypair::KeypairBehavior;
use soroban_client::network::{NetworkPassphrase, Networks};
use soroban_client::server::Options;
use soroban_client::soroban_rpc::GetTransactionStatus;
use soroban_client::transaction::Account;
use soroban_client::transaction::TransactionBehavior;
use soroban_client::transaction_builder::TransactionBuilder;
use soroban_client::transaction_builder::TransactionBuilderBehavior;
use soroban_client::transaction_builder::TIMEOUT_INFINITE;
use soroban_client::{keypair::Keypair, server::Server};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize server connection
    let server = Server::new(
        "https://soroban-testnet.stellar.org",
        Options {
            allow_http: None,
            timeout: Some(1000),
            headers: None,
        },
    )
    .expect("Cannot create Server");

    // Set up source account
    let source_secret_key = "SCZQNYPL4LIZWJBM45R3MBMYX4PXRBZJYJGFI6EPBCRGJTVQW2SEDYO2"; // GDIIRYKAHQJJEGC6DAIWTSDT5TX6OASPT3BE4QO72DXFBR7W43HKUHCL
    let source_keypair = Keypair::from_secret(source_secret_key).expect("Invalid secret key");
    let source_public_key = "GDIIRYKAHQJJEGC6DAIWTSDT5TX6OASPT3BE4QO72DXFBR7W43HKUHCL";

    // Get account information from server
    let account_data = server.get_account(source_public_key).await?;
    let source_account = Rc::new(RefCell::new(
        Account::new(source_public_key, &account_data.sequence_number()).unwrap(),
    ));

    // Contract interaction transaction
    let contract_id = "CAZWWALXKM4OC7FIQZNMZXXZM3Y2ENK3IDKQFU5RLG5VORTUU5ZWW5QY";
    let contract = contract::Contracts::new(contract_id).unwrap();

    let mut contract_tx =
        TransactionBuilder::new(source_account.clone(), Networks::testnet(), None)
            .fee(1000000_u32)
            .add_operation(contract.call("increment", None))
            .set_timeout(TIMEOUT_INFINITE)?
            .build();

    contract_tx = server
        .prepare_transaction(contract_tx, Networks::testnet())
        .await
        .unwrap();
    // let before_signing = contract_tx.to_envelope().unwrap().to_xdr_base64(Limits::none());
    // println!("Before Signing {:?}", before_signing);

    // Sign the contract transaction
    contract_tx.sign(&[source_keypair.clone()]);
    // let after_signing = contract_tx.to_envelope().unwrap().to_xdr_base64(Limits::none());
    // println!("After Signing {:?}", after_signing);

    match server.send_transaction(contract_tx).await {
        Ok(response) => {
            println!("Transaction sent successfully");
            println!("Transaction hash: {}", response.hash);

            // Start polling for transaction completion
            let hash = response.hash.clone();

            loop {
                let response = server.get_transaction(&hash).await;
                if let Ok(tx_result) = response {
                    match tx_result.status {
                        GetTransactionStatus::SUCCESS => {
                            println!("Transaction successful!");
                            if let Some(ledger) = tx_result.ledger {
                                println!("Confirmed in ledger: {}", ledger);
                            }
                            if let Some((meta, Some(return_value))) = tx_result.get_result_meta() {
                                println!("Return value: {:?}", return_value);
                                println!("Transaction metadata: {:?}", meta);
                            }
                            break;
                        }
                        GetTransactionStatus::NOT_FOUND => {
                            println!(
                                "Waiting for transaction confirmation... Latest ledger: {}",
                                tx_result.latest_ledger
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                        GetTransactionStatus::FAILED => {
                            if let Some(result) = tx_result.get_result() {
                                eprintln!("Transaction failed with result: {:?}", result);
                            } else {
                                eprintln!("Transaction failed without result XDR");
                            }
                            break;
                        }
                    }
                } else {
                    eprintln!("Error getting transaction status: {:?}", response);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to send transaction: {}", e);
        }
    }

    Ok(())
}
