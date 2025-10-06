use std::{cell::RefCell, rc::Rc, time::Duration};

use soroban_client::{
    account::{Account, AccountBehavior},
    asset::{Asset, AssetBehavior},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
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

    if server
        .wait_transaction(hash, Duration::from_secs(15))
        .await
        .is_err()
    {
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

    if server
        .wait_transaction(hash, Duration::from_secs(15))
        .await
        .is_err()
    {
        return Err("Failed to create account".into());
    }

    Ok(())
}
