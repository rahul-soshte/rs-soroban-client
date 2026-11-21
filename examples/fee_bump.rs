//! Alice signs a zero-fee payment; Bob sponsors it without Alice signing again.
//! Run: cargo run --example fee_bump
//! Uses two fresh Friendbot-funded testnet accounts, not mainnet or existing keys.

use std::time::Duration;

use soroban_client::{
    asset::{Asset, AssetBehavior},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{Operation, ONE},
    soroban_rpc::{SendTransactionStatus, TransactionStatus},
    transaction::{
        FeeBumpTransaction, ParsedTransaction, Transaction, TransactionBehavior,
        TransactionBuilder, TransactionBuilderBehavior,
    },
    xdr::{
        InnerTransactionResultResult, Limits, OperationResult, OperationResultTr, PaymentResult,
        TransactionResultResult, WriteXdr,
    },
    account::Account,
    error::Error,
    Options, Pagination, Server,
};

use tokio::time::{sleep, timeout};

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";


pub async fn connect_testnet() -> Result<(Server, i64)> {
    let rpc = Server::new(
        TESTNET_RPC,
        Options {
            timeout: 20,
            ..Default::default()
        },
    )?;
    if rpc.get_network().await?.passphrase.as_deref() != Some(Networks::testnet()) {
        return Err("refusing to fund or submit outside testnet".into());
    }

    // Read the ledger base fee instead of assuming that testnet uses 100.
    let latest = rpc.get_latest_ledger().await?;
    let ledgers = rpc
        .get_ledgers(Pagination::From(latest.sequence), 1_u32)
        .await?;
    let header = ledgers
        .ledgers
        .first()
        .and_then(|ledger| ledger.to_header())
        .ok_or("RPC did not return a ledger header")?
        .header;
    if header.ledger_seq != latest.sequence || header.base_fee == 0 {
        return Err("RPC returned an unexpected ledger or a zero base fee".into());
    }
    let base = i64::from(header.base_fee);
    Ok((
        rpc,
        base
    ))
}

pub async fn fund_account(rpc: &Server, key: &Keypair) -> Result<Account> {
    // Friendbot may close its ledger before the RPC has ingested the account.
    // Retry only the lookup, not the funding request.
    match rpc.request_airdrop(&key.public_key()).await {
        Ok(account) => return Ok(account),
        Err(Error::AccountNotFound) => {}
        Err(error) => return Err(error.into()),
    }
    timeout(Duration::from_secs(30), async {
        loop {
            match rpc.get_account(&key.public_key()).await {
                Ok(account) => return Ok(account),
                Err(Error::AccountNotFound) => sleep(Duration::from_secs(1)).await,
                Err(error) => return Err(error),
            }
        }
    })
    .await?
    .map_err(Into::into)
}

async fn sponsor_payment() -> Result {
    let (rpc, base_fee) = connect_testnet().await?;
    let alice = Keypair::random()?;
    let bob = Keypair::random()?;
    println!("Alice (payment source): {}", alice.public_key());
    println!("Bob (recipient and fee payer): {}", bob.public_key());
    let mut alice_account = fund_account(&rpc, &alice).await?;
    fund_account(&rpc, &bob).await?;

    // Alice signs a 1 XLM payment with no fee. 
    let payment = Operation::new()
        .payment(&bob.public_key(), &Asset::native(), ONE)
        .map_err(|error| format!("cannot build payment: {error:?}"))?;
    let mut inner = TransactionBuilder::new(&mut alice_account, Networks::testnet(), None)
        .fee(0_u32)
        .add_operation(payment)
        .set_timeout(180)?
        .build();
    inner.sign(std::slice::from_ref(&alice));
    let inner_hash = inner.try_hash()?;
    let signed_inner = inner.try_to_envelope()?;
    println!(
        "Alice signed a zero-fee 1 XLM payment. Inner hash: {}",
        hex::encode(inner_hash)
    );

    // Bob supplies the TOTAL outer fee and signs only
    // the outer transaction. Alice's signed envelope is preserved.
    let offered = base_fee * 2; // Remove one and get an error
    let mut bump = FeeBumpTransaction::new(
        &bob.public_key(),
        offered,
        signed_inner.clone(),
        Networks::testnet(),
        base_fee,
    )?;
    bump.sign(std::slice::from_ref(&bob))?;
    let outer_hash = bump.hash()?;
    println!(
        "Bob signed the fee bump.  {} stroops",
        offered
    );
    println!("Outer hash: {}", hex::encode(outer_hash));

    // A recipient can read the base64 envelope and submit it directly.
    let encoded = bump.to_envelope().to_xdr_base64(Limits::none())?;
    let parsed = Transaction::from_xdr_envelope(&encoded, Networks::testnet())?;
    let ParsedTransaction::FeeBump(recovered) = &parsed else {
        return Err("decoded envelope is not a fee bump".into());
    };
    if recovered.hash()? != outer_hash
        || recovered.inner_hash()? != inner_hash
        || recovered.inner_transaction().try_to_envelope()? != signed_inner
    {
        return Err("envelope parsing changed the signed transaction".into());
    }
    println!("Base64 round trip preserved Alice's signed transaction; she did not sign again.");

    let submitted = rpc.send_transaction(&parsed).await?;
    if submitted.hash != hex::encode(outer_hash)
        || !matches!(
            submitted.status,
            SendTransactionStatus::Pending | SendTransactionStatus::Duplicate
        )
    {
        return Err(format!("fee bump was not accepted: {submitted:?}").into());
    }
    println!("Submitted. Waiting for confirmation using the OUTER hash...");
    let confirmed = timeout(
        Duration::from_secs(120),
        rpc.wait_transaction(&submitted.hash, Duration::from_secs(120)),
    )
    .await?
    .map_err(|(error, last)| format!("confirmation failed: {error}; last response: {last:?}"))?;
    if confirmed.status != TransactionStatus::Success || confirmed.fee_bump != Some(true) {
        return Err(format!("fee bump did not succeed: {confirmed:?}").into());
    }
    let ledger = confirmed.ledger.ok_or("missing confirmation ledger")?;
    let result = confirmed
        .to_result()
        .ok_or("missing confirmed transaction result")?;
    let TransactionResultResult::TxFeeBumpInnerSuccess(inner_result) = &result.result else {
        return Err(format!("unexpected fee-bump result: {result:?}").into());
    };
    let InnerTransactionResultResult::TxSuccess(operations) = &inner_result.result.result else {
        return Err(format!("inner payment failed: {inner_result:?}").into());
    };
    if inner_result.transaction_hash.0 != inner_hash
        || operations.len() != 1
        || !matches!(
            &operations[0],
            OperationResult::OpInner(OperationResultTr::Payment(PaymentResult::Success))
        )
    {
        return Err("confirmed inner result does not match Alice's payment".into());
    }
    println!(
        "Confirmed in ledger {ledger}: Alice's payment succeeded, Bob paid {} stroops.",
        result.fee_charged
    );
    println!(
        "https://stellar.expert/explorer/testnet/tx/{}",
        submitted.hash
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result {
    // Also bounds Friendbot requests and the wait helper's exponential backoff.
    timeout(Duration::from_secs(240), sponsor_payment()).await?
}
