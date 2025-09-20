use std::{cell::RefCell, rc::Rc, time::Duration};

use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    soroban_rpc::{
        GetTransactionResponse, SendTransactionResponse, SendTransactionStatus, TransactionStatus,
    },
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    Options, Server,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = soroban_client::Server::new(server_url, Options::default())?;

    let source_keypair = Keypair::random()?;
    let source_public_key = &source_keypair.public_key();
    let signers = [source_keypair];

    // Get account information from server
    let account_data = server.request_airdrop(source_public_key).await?;
    let source_account = Rc::new(RefCell::new(Account::new(
        source_public_key,
        &account_data.sequence_number(),
    )?));

    //
    // Calling the increment method of the contract
    let contract_addr = "CBU3OHKZ2BHOHK5VMG3HBWIW3PBQHZLNMHNJUGM23W5NBFA75JMMWAVT";
    let contract = Contracts::new(contract_addr).unwrap();
    let tx = TransactionBuilder::new(source_account, Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(contract.call(
            "increment",
            Some(vec![
                Address::account(signers[0].raw_public_key())?.to_sc_val()?,
                3u32.into(),
            ]),
        ))
        .build();

    //
    // Preparing the transaction, this will call `server.simulate_transaction` and
    // `assemble_transaction` to enhance the transaction with the soroban data and auths
    let ptxr = server.prepare_transaction(&tx).await;
    let mut ptx = match ptxr {
        Ok(p) => p,
        Err(e) => {
            // Manage errors here
            return Err(e.into());
        }
    };

    //
    // Sign the transaction with the source account
    ptx.sign(&signers);

    println!("> Calling increment on contract {contract_addr}",);
    let response = server.send_transaction(ptx).await?;

    let hash = &response.hash;
    println!(">> Tx hash: {hash}");
    let counter: u32 = if let Some(tx_result) = wait_success(&server, response).await {
        // On success we can extract the returned value
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No result meta");
        ret_val
            .expect("None returned value")
            .try_into()
            .expect("Return value is not u32")
    } else {
        return Err("Failed to create contract".into());
    };
    println!(">> Counter: {counter}",);
    println!();

    Ok(())
}

//
// Polling the `get_transaction` until the transaction is found in Success or Failed
async fn wait_success(
    server: &Server,
    response: SendTransactionResponse,
) -> Option<GetTransactionResponse> {
    if response.status != SendTransactionStatus::Error {
        let mut count = 0;
        loop {
            let response = server.get_transaction(&response.hash).await;
            if let Ok(tx_result) = response {
                match tx_result.status {
                    TransactionStatus::Success => {
                        if let Some(ledger) = tx_result.ledger {
                            println!(">> Confirmed in ledger: {}", ledger);
                        }
                        return Some(tx_result);
                    }
                    TransactionStatus::NotFound => {
                        count += 1;

                        if count > 6 {
                            println!(
                                ">> Waiting for transaction confirmation... Latest ledger: {}",
                                tx_result.latest_ledger
                            );
                        }

                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    TransactionStatus::Failed => {
                        if let Some(result) = tx_result.to_result() {
                            eprintln!("Transaction failed with result: {:?}", result);
                        } else {
                            eprintln!("Transaction failed without result XDR");
                        }
                        return None;
                    }
                }
            } else {
                eprintln!("! Error getting transaction status: {:?}", response);
            }
        }
    }
    None
}
