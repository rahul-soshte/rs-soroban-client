//! Fee-bump a transaction on testnet.
//!
//! Account A signs a payment. Account B wraps it in a fee bump, signs, and pays the fee.
//! The example then confirms the network applied the transaction as a fee bump, under
//! the fee-bump hash, and reads the inner transaction back from the confirmed envelope.
//!
//! Run with: `cargo run --example fee_bump`
use std::time::Duration;

use soroban_client::{
    account::{Account, AccountBehavior},
    asset::{Asset, AssetBehavior},
    fee_bump_transaction::{FeeBumpTransaction, GenericTransaction},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr::{Limits, WriteXdr},
    Options, Server,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let network = Networks::testnet();

    // A: the inner transaction's source. B: the fee source. Different accounts.
    let inner_kp = Keypair::random()?;
    let fee_kp = Keypair::random()?;
    let inner_account = server.request_airdrop(&inner_kp.public_key()).await?;
    server.request_airdrop(&fee_kp.public_key()).await?;
    println!("Inner source (A): {}", inner_kp.public_key());
    println!("Fee source   (B): {}", fee_kp.public_key());

    // A signs a 1 XLM payment to B at the minimum fee.
    let mut source = Account::new(&inner_kp.public_key(), &inner_account.sequence_number())?;
    let mut inner_tx = TransactionBuilder::new(&mut source, network, None)
        .fee(100u32)
        .add_operation(
            Operation::new()
                .payment(&fee_kp.public_key(), &Asset::native(), operation::ONE)
                .map_err(|e| format!("{e:?}"))?,
        )
        .set_timeout(300)?
        .build();
    inner_tx.sign(&[inner_kp.clone()]);

    // A fee below the network minimum is refused here, before anything is sent.
    let too_low = FeeBumpTransaction::new(&fee_kp.public_key(), 50, &inner_tx);
    println!("Base fee 50 -> {}", too_low.unwrap_err());

    // B wraps it, paying 1000 stroops per operation (2 ops: the payment + the bump).
    let mut fee_bump = FeeBumpTransaction::new(&fee_kp.public_key(), 1_000, &inner_tx)?;
    fee_bump.sign(&[fee_kp.clone()])?;
    let fee_bump_hash = hex::encode(fee_bump.hash());
    let inner_hash = hex::encode(fee_bump.inner_transaction_hash());
    println!("Fee-bump hash: {fee_bump_hash}");
    println!("Inner hash:    {inner_hash}");

    let sent = server.send_transaction(fee_bump.clone()).await?;
    assert_eq!(sent.hash, fee_bump_hash, "RPC reports the fee-bump hash");

    let confirmed = server
        .wait_transaction(&sent.hash, Duration::from_secs(30))
        .await
        .map_err(|(e, _)| e)?;
    println!(
        "Status: {:?}, ledger {:?}",
        confirmed.status, confirmed.ledger
    );
    assert_eq!(confirmed.fee_bump, Some(true), "applied as a fee bump");

    // Read the confirmed envelope back and recover the inner transaction.
    let envelope_xdr = confirmed
        .to_envelope()
        .ok_or("no envelope in response")?
        .to_xdr_base64(Limits::none())?;
    let parsed = GenericTransaction::from_xdr_envelope(&envelope_xdr, network)?;
    let parsed = parsed.as_fee_bump().ok_or("expected a fee-bump envelope")?;
    assert_eq!(parsed.fee_source(), fee_kp.public_key());
    assert_eq!(hex::encode(parsed.inner_transaction_hash()), inner_hash);
    assert_eq!(
        parsed.inner_transaction().source.as_deref(),
        Some(inner_kp.public_key().as_str())
    );

    // The inner transaction consumed A's sequence number. B paid the fee.
    let a_after = server.get_account(&inner_kp.public_key()).await?;
    println!("A sequence after: {}", a_after.sequence_number());
    println!("Fee bump confirmed; inner transaction recovered from the network.");
    Ok(())
}
