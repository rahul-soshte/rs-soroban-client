use std::{cell::RefCell, rc::Rc, time::Duration};

use soroban_client::{
    account::{Account, AccountBehavior},
    asset::{Asset, AssetBehavior},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
    soroban_rpc::{SendTransactionStatus, TransactionStatus},
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr, Options, Server,
};

const TESTNET_USDC_ISSUER: &str = "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5";

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = soroban_client::Server::new(server_url, Options::default())?;

    //
    // Generate keypairs
    //
    let parent = Keypair::random()?;
    println!("Secret: {}", parent.secret_key()?);
    println!("Public: {}", parent.public_key());

    //
    // Fund a new account via friendbot
    //
    let account_data = server.request_airdrop(&parent.public_key()).await?;
    println!("SUCCESS! You have a new account:\n{account_data:?}");

    let child = Keypair::random()?;
    let parent_account = Rc::new(RefCell::new(
        Account::new(&parent.public_key(), &account_data.sequence_number()).unwrap(),
    ));

    //
    // Fund a new account via CreateAccount operation
    //
    let mut builder = TransactionBuilder::new(parent_account.clone(), Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(
        Operation::new()
            .create_account(&child.public_key(), operation::ONE * 5)
            .unwrap(),
    );

    let mut tx = builder.build();
    tx.sign(&[parent]);

    let response = server.send_transaction(tx).await?;

    //
    // Polling for transaction completion
    //
    let hash = &response.hash;
    println!("Tx hash: {}", hash);
    if !wait_success(&server, hash, response.status).await {
        return Err("Failed to create account".into());
    }

    //
    // Fetch native and USDC balances for account
    //
    let account_id = child.xdr_account_id();
    let ledger_key = xdr::LedgerKey::Account(xdr::LedgerKeyAccount { account_id });
    let response = server.get_ledger_entries(vec![ledger_key]).await?;
    if let xdr::LedgerEntryData::Account(account) = response.entries.unwrap()[0].to_data() {
        // Convert the balance from stroops
        let balance = account.balance / operation::ONE;
        println!("XLM: {balance}");
    }

    let account_id = child.xdr_account_id();
    let asset = Asset::new("USDC", Some(TESTNET_USDC_ISSUER))?.into();
    let ledger_key = xdr::LedgerKey::Trustline(xdr::LedgerKeyTrustLine { account_id, asset });
    let response = server.get_ledger_entries(vec![ledger_key]).await?;
    if let Some(entries) = response.entries {
        if !entries.is_empty() {
            if let xdr::LedgerEntryData::Trustline(trustline) = entries[0].to_data() {
                println!("USDC trustline balance (raw): {}", trustline.balance);
            }
        } else {
            println!("No USDC trustline");
        }
    }

    Ok(())
}

async fn wait_success(server: &Server, hash: &str, status: SendTransactionStatus) -> bool {
    if status != SendTransactionStatus::Error {
        loop {
            let response = server.get_transaction(hash).await;
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
