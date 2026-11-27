//! Run with a deployed contract exposing `add(a: u32, b: u32) -> u32`:
//! cargo run --features contract-spec --example typed_contract -- C...
use soroban_client::{
    contract_client::{CallError, Client},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    Options, Server,
};

#[derive(Debug, serde::Deserialize)]
enum ArithmeticError {
    Overflow,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id = std::env::args()
        .nth(1)
        .ok_or("usage: typed_contract CONTRACT_ID")?;
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let key = Keypair::random()?;
    let source = server.request_airdrop(&key.public_key()).await?;
    let client = Client::new(&server, &id).await?;
    println!("Metadata: {:?}", client.metadata());
    println!("Unverified SEP claims: {:?}", client.sep_claims()?);
    let call = client.call("add", &(2u32, 3u32))?;
    match call
        .try_simulate::<u32, ArithmeticError>(&source, Networks::testnet())
        .await
    {
        Ok(value) => {
            assert_eq!(value, 5);
            println!("Simulated result: {value}");
        }
        Err(CallError::Contract(ArithmeticError::Overflow)) => {
            println!("Contract reported overflow")
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
