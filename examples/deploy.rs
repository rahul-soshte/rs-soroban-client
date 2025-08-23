use std::{cell::RefCell, rc::Rc, time::Duration};

use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    soroban_rpc::{
        GetTransactionResponse, SendTransactionResponse, SendTransactionStatus, TransactionStatus,
    },
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr, Options, Server,
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

    let wasm = std::fs::read("./examples/counter.wasm").expect("Cannot read contract");

    //
    // Uploading the WASM executable
    //
    let upload = Operation::new()
        .upload_wasm(&wasm, None)
        .expect("Cannot create upload_wasm operation");

    let mut builder = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(upload);

    let tx = builder.build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&[source_keypair.clone()]);
    let response = server.send_transaction(ptx).await?;

    dbg!(&response);
    let hash = response.hash.clone();
    println!("Tx hash: {}", hash);

    let mut wasm_hash = [0; 32];
    if let Some(tx_result) = wait_success(&server, hash, response).await {
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta");
        println!("Wasm hash: {:?}", ret_val);
        if let Some(xdr::ScVal::Bytes(xdr::ScBytes(bytes))) = ret_val {
            wasm_hash = *bytes.to_vec().last_chunk::<32>().unwrap();
        }
    } else {
        return Err("Failed to create account".into());
    }

    //
    // Create the contract for the uploaded WASM
    //
    let create_contract = Operation::new()
        .create_contract(source_public_key, wasm_hash, None, None, [].into())
        .expect("Cannot create op");

    let mut builder = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(create_contract);

    let tx = builder.build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&[source_keypair.clone()]);
    let response = server.send_transaction(ptx).await?;

    dbg!(&response);
    let hash = response.hash.clone();
    println!("Tx hash: {}", hash);

    let contract_addr = if let Some(tx_result) = wait_success(&server, hash, response).await {
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta");
        if let Some(xdr::ScVal::Address(addr)) = ret_val {
            Address::from_sc_address(&addr).unwrap()
        } else {
            return Err("Failed to create contract".into());
        }
    } else {
        return Err("Failed to create contract".into());
    };

    println!("Contract id: {}", contract_addr.to_string());

    //
    // Calling the inc method of the contract
    //
    let contract = Contracts::new(&contract_addr.to_string()).unwrap();

    let mut builder = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None);
    builder.fee(1000u32);
    builder.add_operation(contract.call("inc", None));

    let tx = builder.build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&[source_keypair.clone()]);
    let response = server.send_transaction(ptx).await?;

    dbg!(&response);
    // Start polling for transaction completion
    let hash = response.hash.clone();
    println!("Tx hash: {}", hash);

    if let Some(tx_result) = wait_success(&server, hash, response).await {
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta");

        println!("Counter: {:?}", ret_val); // should be 1 since it's a new contract
    } else {
        return Err("Failed to create contract".into());
    }

    Ok(())
}

async fn wait_success(
    server: &Server,
    hash: String,
    response: SendTransactionResponse,
) -> Option<GetTransactionResponse> {
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
                        return Some(tx_result);
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
                        return None;
                    }
                }
            } else {
                eprintln!("Error getting transaction status: {:?}", response);
            }
        }
    }
    None
}
