//! SEP-29: refuse payments without a memo to accounts that require one.
//!
//! Runs against testnet:
//! 1. funds a sender and an "exchange" account with friendbot,
//! 2. the exchange sets the `config.memo_required = "1"` data entry,
//! 3. a payment without a memo is refused before it reaches the network,
//! 4. the same payment with a memo, and one to the exchange's muxed (M...) address, go through.
//!
//! Spec: https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0029.md

use std::time::Duration;

use soroban_client::{
    asset::{Asset, AssetBehavior},
    error::Error,
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
    soroban_rpc::TransactionStatus,
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    utils::decode_encode_muxed_account::{encode_muxed_account, encode_muxed_account_to_address},
    Options, Server,
};

async fn submit(
    server: &Server,
    tx: soroban_client::transaction::Transaction,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = server.send_transaction(tx).await?;
    let result = server
        .wait_transaction(&response.hash, Duration::from_secs(30))
        .await
        .map_err(|(e, _)| e)?;
    if result.status != TransactionStatus::Success {
        return Err(format!("transaction {} failed", response.hash).into());
    }
    println!("  succeeded: {}", response.hash);
    Ok(())
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;

    let sender = Keypair::random()?;
    let exchange = Keypair::random()?;
    let mut sender_account = server.request_airdrop(&sender.public_key()).await?;
    let mut exchange_account = server.request_airdrop(&exchange.public_key()).await?;
    println!("sender:   {}", sender.public_key());
    println!("exchange: {}", exchange.public_key());

    // The exchange marks its account as memo-required.
    println!("exchange sets config.memo_required = \"1\"");
    let mut builder = TransactionBuilder::new(&mut exchange_account, Networks::testnet(), None);
    builder.fee(1000u32).add_operation(
        Operation::new()
            .manage_data("config.memo_required", Some(&b"1".to_vec()))
            .expect("manage_data operation"),
    );
    let mut tx = builder.build();
    tx.sign(&[exchange.clone()]);
    submit(&server, tx).await?;

    let payment = || {
        Operation::new()
            .payment(&exchange.public_key(), &Asset::native(), operation::ONE)
            .expect("payment operation")
    };

    // 1. No memo: refused locally, never sent.
    println!("payment without memo");
    let mut builder = TransactionBuilder::new(&mut sender_account, Networks::testnet(), None);
    builder.fee(1000u32).add_operation(payment());
    let mut tx = builder.build();
    tx.sign(&[sender.clone()]);
    match server.send_transaction(tx).await {
        Err(Error::AccountRequiresMemo {
            account_id,
            operation_index,
        }) => {
            assert_eq!(account_id, exchange.public_key());
            println!("  refused: {account_id} requires a memo (operation {operation_index})");
        }
        other => return Err(format!("expected AccountRequiresMemo, got {other:?}").into()),
    }

    // The refused transaction consumed no sequence number on chain: reload the account.
    let mut sender_account = server.get_account(&sender.public_key()).await?;

    // 2. With a memo: allowed.
    println!("payment with memo");
    let mut builder = TransactionBuilder::new(&mut sender_account, Networks::testnet(), None);
    builder
        .fee(1000u32)
        .add_operation(payment())
        .add_memo("user-1234");
    let mut tx = builder.build();
    tx.sign(&[sender.clone()]);
    submit(&server, tx).await?;

    // 3. To the exchange's muxed address, no memo: allowed (the muxed id identifies the user).
    let muxed =
        encode_muxed_account_to_address(&encode_muxed_account(&exchange.public_key(), "1234"));
    println!("payment to muxed {muxed} without memo");
    let mut builder = TransactionBuilder::new(&mut sender_account, Networks::testnet(), None);
    builder.fee(1000u32).add_operation(
        Operation::new()
            .payment(&muxed, &Asset::native(), operation::ONE)
            .expect("payment operation"),
    );
    let mut tx = builder.build();
    tx.sign(&[sender.clone()]);
    submit(&server, tx).await?;

    println!("done");
    Ok(())
}
