use std::rc::Rc;
use std::cell::RefCell;
use soroban_client::network::{Networks, NetworkPassphrase};
use soroban_client::server::Durability;
#[allow(warnings)]
use soroban_client::server::Options;
use soroban_client::contract_spec::native_to_sc_val;
use soroban_client::transaction::Account;
use soroban_client::transaction_builder::TransactionBuilder;
use soroban_client::{server::Server, keypair::Keypair};
use soroban_client::keypair::KeypairBehavior;
use soroban_client::transaction_builder::TransactionBuilderBehavior;
use soroban_client::account::AccountBehavior;
use stellar_baselib::{xdr, contract};
use stellar_xdr::next::{ScVec, WriteXdr, Limits, ScContractInstance};
use stellar_xdr::next::{HostFunction, ScSymbol, InvokeContractArgs, Hash, StringM, ScString, ScVal, ReadXdr, ScSpecType, ContractDataDurability};
use stellar_xdr::next::ScAddress;
use stellar_xdr::next::ScAddress::Contract;
use std::str::FromStr;
use stellar_baselib::transaction::TransactionBehavior;
use stellar_baselib::contract::ContractBehavior;
use soroban_client::transaction_builder::TIMEOUT_INFINITE;
use soroban_client::operation::PaymentOpts;
use soroban_client::operation::Operation;
use soroban_client::asset::Asset;
use soroban_client::asset::AssetBehavior;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize server connection
    let server = Server::new(
        "https://soroban-testnet.stellar.org",
        Options {
            allow_http: None,
            timeout: Some(1000),
            headers: None,
        }
    );

    // Set up source account
    let source_secret_key = "SBQ2476EDDHYVYPCVRLSOL2XPHF3ALPZ7LN36J54D7CZKNBVZOPO32LP";
    let source_keypair = Keypair::from_secret(source_secret_key).expect("Invalid secret key");
    let source_public_key = "GCLLIMRLKE5NXUHAKG5WO5P65KARTB6TYPVGZVFYCJGU7SFUBW7C23KI";

    // Get account information from server
    let account_data = server.get_account(source_public_key).await?;
    let source_account = Rc::new(RefCell::new(
        Account::new(source_public_key, &account_data.sequence_number()).unwrap()
    ));

    // Contract interaction transaction
    let contract_id = "CCSE2AN2S4RLMMXJY5FRYQ4YN6UGG54LJT3HPWGGISMJI4OAUYOY6AVR";
    let contract = contract::Contracts::new(contract_id).unwrap();
    
    let mut contract_tx = TransactionBuilder::new(
        source_account.clone(),
        Networks::testnet(),
        None
    )
    .fee(100_u32)
    .add_operation(contract.call("increment", None))
    .set_timeout(TIMEOUT_INFINITE)?
    .build();

    // Sign the contract transaction
    contract_tx.sign(&[source_keypair.clone()]);
    
    // Payment transaction
    let destination = "GAAOFCNYV2OQUMVONXH2DOOQNNLJO7WRQ7E4INEZ7VH7JNG7IKBQAK5D";
    let amount = "2000";
    
    let mut payment_tx = TransactionBuilder::new(
        source_account.clone(),
        Networks::testnet(),
        None
    )
    .fee(100_u32)
    .add_operation(
        Operation::payment(PaymentOpts {
            destination: destination.to_owned(),
            asset: Asset::native(),
            amount: amount.to_owned(),
            source: None,
        })?
    )
    .add_memo("Happy birthday!")
    .set_timeout(TIMEOUT_INFINITE)?
    .build();

    // Sign the payment transaction
    // payment_tx.sign(&[source_keypair.clone()]);
  
    

    let val = contract_tx.to_envelope().unwrap().to_xdr_base64(Limits::none());
    println!("{:?}", val);

    // let val = match server.send_transaction(payment_tx).await {
    //     Ok(transaction_result) => {
    //         println!("{:?}", transaction_result);
    //     }
    //     Err(err) => {
    //         eprintln!("{:?}", err);
    //     }
    // };

    Ok(())
}