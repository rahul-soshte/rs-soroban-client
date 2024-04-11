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

    transaction = server.prepare_transaction(transaction, Some(Networks::testnet())).await.unwrap();


}
