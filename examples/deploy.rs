use std::time::Duration;

use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    soroban_rpc::TransactionStatus,
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr, Options, Server,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = Server::new(server_url, Options::default())?;

    let source_keypair = Keypair::random()?;
    let source_public_key = &source_keypair.public_key();
    let signers = [source_keypair];

    // Get account information from server
    let account_data = server.request_airdrop(source_public_key).await?;
    let mut source_account = Account::new(
        source_public_key,
        &account_data.sequence_number(),
    )?;

    let wasm = std::fs::read("./examples/soroban_auth_contract.wasm")?;

    //
    // Uploading the WASM executable
    //
    let upload = Operation::new()
        .upload_wasm(&wasm, None)
        .expect("Cannot create upload_wasm operation");
    let tx = TransactionBuilder::new(&mut source_account, Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(upload)
        .build();

    let mut ptx = server.prepare_transaction(&tx).await?;
    ptx.sign(&signers);

    println!("> Uploading WASM executable");
    let response = server.send_transaction(ptx).await?;

    let hash = response.hash;
    println!(">> Tx hash: {hash}");
    let wasm_hash = match server
        .wait_transaction(&hash, Duration::from_secs(15))
        .await
    {
        Ok(tx_result) if tx_result.status == TransactionStatus::Success => {
            let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta found");
            if let Some(scval) = ret_val {
                let bytes: Vec<u8> = scval.try_into().expect("Cannot convert ScVal to Vec<u8>");
                *bytes.last_chunk::<32>().expect("Not 32 bytes")
            } else {
                return Err(">> None return value".into());
            }
        }
        _ => {
            println!(">> Failed to upload the WASM executable");
            return Err(">> Failed to upload the wasm".into());
        }
    };
    println!(">> Wasm hash: {}", hex::encode(wasm_hash));
    println!();

    //
    // Create the contract for the uploaded WASM
    //
    let create_contract = Operation::new()
        .create_contract(source_public_key, wasm_hash, None, None, [].into())
        .expect("Cannot create create_contract operation");
    let tx = TransactionBuilder::new(&mut source_account, Networks::testnet(), None)
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

    let hash = response.hash;
    println!(">> Tx hash: {hash}");
    let contract_addr = match server
        .wait_transaction(&hash, Duration::from_secs(15))
        .await
    {
        Ok(tx_result) if tx_result.status == TransactionStatus::Success => {
            let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta");
            if let Some(xdr::ScVal::Address(addr)) = ret_val {
                Address::from_sc_address(&addr).unwrap()
            } else {
                return Err("Failed to create contract".into());
            }
        }
        _ => return Err("Failed to create contract".into()),
    };
    println!(">> Contract id: {}", contract_addr.to_string());
    println!();

    //
    // Calling the increment method of the contract
    //
    let contract = Contracts::new(&contract_addr.to_string()).unwrap();
    let tx = TransactionBuilder::new(&mut source_account, Networks::testnet(), None)
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

    let hash = response.hash;
    println!(">> Tx hash: {hash}");
    let counter: u32 = match server
        .wait_transaction(&hash, Duration::from_secs(15))
        .await
    {
        Ok(tx_result) if tx_result.status == TransactionStatus::Success => {
            let (_meta, ret_val) = tx_result.to_result_meta().expect("No result meta");
            ret_val
                .expect("None returned value")
                .try_into()
                .expect("Return value is not u32")
        }
        _ => return Err("Failed to create contract".into()),
    };
    println!(">> Counter: {counter}",);
    println!();

    Ok(())
}
