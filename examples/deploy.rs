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

    let wasm = std::fs::read("./examples/soroban_auth_contract.wasm")?;

    //
    // Uploading the WASM executable
    //
    let upload = Operation::new()
        .upload_wasm(&wasm, None)
        .expect("Cannot create upload_wasm operation");
    let tx = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(upload)
        .build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&signers);

    println!("> Uploading WASM executable");
    let response = server.send_transaction(ptx).await?;

    let hash = &response.hash;
    println!(">> Tx hash: {hash}");
    let wasm_hash = if let Some(tx_result) = wait_success(&server, response).await {
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta found");
        if let Some(scval) = ret_val {
            let bytes: Vec<u8> = scval.try_into().expect("Cannot convert ScVal to Vec<u8>");
            *bytes.last_chunk::<32>().expect("Not 32 bytes")
        } else {
            return Err(">> None return value".into());
        }
    } else {
        println!(">> Failed to upload the WASM executable");
        return Err(">> Failed to upload the wasm".into());
    };
    println!(">> Wasm hash: {}", hex::encode(wasm_hash));
    println!();

    //
    // Create the contract for the uploaded WASM
    //
    let create_contract = Operation::new()
        .create_contract(source_public_key, wasm_hash, None, None, [].into())
        .expect("Cannot create create_contract operation");
    let tx = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(create_contract)
        .build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&signers);

    println!(
        "> Creating the contract for WASM hash {}",
        hex::encode(wasm_hash)
    );
    let response = server.send_transaction(ptx).await?;

    let hash = &response.hash;
    println!(">> Tx hash: {hash}");
    let contract_addr = if let Some(tx_result) = wait_success(&server, response).await {
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta");
        if let Some(xdr::ScVal::Address(addr)) = ret_val {
            Address::from_sc_address(&addr).unwrap()
        } else {
            return Err("Failed to create contract".into());
        }
    } else {
        return Err("Failed to create contract".into());
    };
    println!(">> Contract id: {}", contract_addr.to_string());
    println!();

    //
    // Calling the increment method of the contract
    //
    let contract = Contracts::new(&contract_addr.to_string()).unwrap();
    let tx = TransactionBuilder::new(source_account.clone(), Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(contract.call(
            "increment",
            Some(vec![
                Address::account(signers[0].raw_public_key())?.to_sc_val()?,
                3u32.into(),
            ]),
        ))
        .build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&signers);

    println!(
        "> Calling increment on contract {}",
        contract_addr.to_string()
    );
    let response = server.send_transaction(ptx).await?;

    let hash = &response.hash;
    println!(">> Tx hash: {hash}");
    let counter: u32 = if let Some(tx_result) = wait_success(&server, response).await {
        let (_meta, ret_val) = tx_result.to_result_meta().expect("No result meta");
        ret_val
            .expect("None returned value")
            .try_into()
            .expect("Return value is not u32")
    } else {
        return Err("Failed to create contract".into());
    };
    println!(">> Counter: {counter}",); // should be 1 since it's a new contract
    println!();

    Ok(())
}

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
