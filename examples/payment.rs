use std::{cell::RefCell, rc::Rc, time::Duration};

use soroban_client::{
    account::{Account, AccountBehavior},
    asset::{Asset, AssetBehavior},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
    soroban_rpc::{SendTransactionResponse, SendTransactionStatus, TransactionStatus},
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    Options, Server,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server =
        soroban_client::Server::new(server_url, Options::default()).expect("Cannot create server");

    let source_keypair = Keypair::random().unwrap();
    let source_public_key = &source_keypair.public_key();

    // Get account information from server
    let account_data = server.request_airdrop(source_public_key).await?;
    let source_account = Rc::new(RefCell::new(
        Account::new(source_public_key, &account_data.sequence_number()).unwrap(),
    ));

    let to_create_keypair = Keypair::random().unwrap();
    let to_create_public_key = &to_create_keypair.public_key();

    println!("Source: {}", source_public_key);
    println!("Destination: {}", to_create_public_key);

    let create_account_op = Operation::new()
        .create_account(to_create_public_key, operation::ONE)
        .expect("Cannot create operation");

    let mut builder = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(create_account_op);

    let mut tx = builder.build();
    tx.sign(&[source_keypair.clone()]);

    let response = server.send_transaction(tx).await?;

    let hash = response.hash.clone();
    println!("Tx hash: {}", hash);

    if !wait_success(&server, hash, response).await {
        return Err("Failed to create account".into());
    }

    let payment = Operation::new()
        .payment(to_create_public_key, &Asset::native(), 100 * operation::ONE)
        .expect("Cannot create payment operation");

    let mut builder = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(payment);

    let mut tx = builder.build();
    tx.sign(&[source_keypair.clone()]);

    let response = server.send_transaction(tx).await?;

    let hash = response.hash.clone();
    println!("Tx hash: {}", hash);

    if !wait_success(&server, hash, response).await {
        return Err("Failed to create account".into());
    }

    Ok(())
}

async fn wait_success(server: &Server, hash: String, response: SendTransactionResponse) -> bool {
    if response.status != SendTransactionStatus::Error {
        loop {
            let response = server.get_transaction(&hash).await;
            if let Ok(tx_result) = response {
                match tx_result.status {
                    TransactionStatus::Success => {
                        println!("Transaction successful!");
                        if let Some(ledger) = tx_result.ledger {
                            println!("Confirmed in ledger: {}", ledger);
                        }
                        return true;
                    }
                    TransactionStatus::NotFound => {
                        println!(
                            "Waiting for transaction confirmation... Latest ledger: {}",
                            tx_result.latest_ledger
                        );
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    TransactionStatus::Failed => {
                        if let Some(result) = tx_result.to_result() {
                            eprintln!("Transaction failed with result: {:?}", result);
                        } else {
                            eprintln!("Transaction failed without result XDR");
                        }
                        return false;
                    }
                }
            } else {
                eprintln!("Error getting transaction status: {:?}", response);
            }
        }
    }
    false
}
