use std::time::Duration;

use soroban_client::{
    account::{Account, AccountBehavior},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
    soroban_rpc::TransactionStatus,
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    Options, Server,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = Server::new(server_url, Options::default()).expect("Cannot create server");

    let source_keypair = Keypair::random().unwrap();
    let source_public_key = &source_keypair.public_key();

    // Get account information from server
    let account_data = server.request_airdrop(source_public_key).await?;
    let mut source_account =
        Account::new(source_public_key, &account_data.sequence_number()).unwrap();

    let to_create_keypair = Keypair::random().unwrap();
    let to_create_public_key = &to_create_keypair.public_key();

    let create_account_op = Operation::new()
        .create_account(to_create_public_key, operation::ONE)
        .expect("Cannot create operation");

    let mut builder = TransactionBuilder::new(&mut source_account, Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(create_account_op);

    let mut tx = builder.build();
    tx.sign(&[source_keypair]);

    let response = server.send_transaction(tx).await?;

    // Start polling for transaction completion
    let hash = response.hash.clone();
    println!("Tx hash: {}", hash);

    match server
        .wait_transaction(&hash, Duration::from_secs(15))
        .await
    {
        Ok(tx_result) => match tx_result.status {
            TransactionStatus::Success => {
                println!("Transaction successful!");
                if let Some(ledger) = tx_result.ledger {
                    println!("Confirmed in ledger: {}", ledger);
                }
                Ok(())
            }
            TransactionStatus::Failed => {
                if let Some(result) = tx_result.to_result() {
                    eprintln!("Transaction failed with result: {:?}", result);
                } else {
                    eprintln!("Transaction failed without result XDR");
                }
                Ok(())
            }
            TransactionStatus::NotFound => {
                eprintln!("Transaction not found");
                Err("Transaction not found".into())
            }
        },
        Err(e) => Err(e.0.to_string().into()),
    }
}
