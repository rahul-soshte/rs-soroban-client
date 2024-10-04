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

// Testnet -> https://soroban-testnet.stellar.org
// Futurenet -> https://rpc-futurenet.stellar.org:443

#[tokio::main]
async fn main() {

    let server = Server::new("https://soroban-testnet.stellar.org", Options{ allow_http: None, timeout: Some(1000), headers: None });
    let source_secret_key = "SCCTADNI4B4FEFELEYEYSDUNQXVTXHRAOEXWWJWHJ57EO3VHGXJFL3TC";
    let source_keypair = Keypair::from_secret(source_secret_key).expect("Invalid secret key");
    let _source_public_key = source_keypair.public_key();
    let _source_public_key = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";

    let public_key = _source_public_key; // Replace with the actual public key
    // let secret_string: &str = source_secret_key; // Replace with the actual secret key
    let contract_id = "CDEJ6E4AGUKHNRXQUKFCPLGNB2GGC4LVRR2GVAY3EQTOLM3FPLBAPEIO"; // Replace with the actual contract ID
    let source_secret_key = "SCCTADNI4B4FEFELEYEYSDUNQXVTXHRAOEXWWJWHJ57EO3VHGXJFL3TC";
    let source_keypair = Keypair::from_secret(source_secret_key).expect("Invalid secret key");
    let _source_public_key = source_keypair.public_key();
    let _source_public_key = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";

    let account = server.get_account(public_key).await.unwrap();
    let fee = 100_u32;
    let contract = contract::Contracts::new(contract_id).unwrap();

    let mut transaction = TransactionBuilder::new(account, Networks::testnet())
        .fee(fee)
        .add_operation(
            contract.call("increment", None),
        )
        .build();
    
    let destination = "GAAOFCNYV2OQUMVONXH2DOOQNNLJO7WRQ7E4INEZ7VH7JNG7IKBQAK5D";
    let asset = Asset::native();
    let amount = "2000";

    let source = Account::new(
        "GBBM6BKZPEHWYO3E3YKREDPQXMS4VK35YLNU7NFBRI26RAN7GI5POFBB",
        "20",
    )
    .unwrap();

    let tx = TransactionBuilder::new(source.clone(), Networks::testnet())
        .fee(100_u32)
        .add_operation(Operation::payment(PaymentOpts {
            destination: destination.to_owned(),
            asset,
            amount: amount.to_owned(),
            source: None,
        }).unwrap())
        .add_memo("Happy birthday!")
        .set_timeout(TIMEOUT_INFINITE)
        .unwrap()
        .build();


    // TODO: Extract the Transaction Envelope XDR and check if its a valid XDR
    println!("{:?}", tx_env);


    // TODO: Sign the Transaction
    // TODO: Send the Transaction    
}
