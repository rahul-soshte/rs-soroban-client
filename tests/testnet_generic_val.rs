//! Opt-in red-phase check against a deployed native fixture. The low-level RPC
//! result must be correct before the typed-client round trip is tested.
#![cfg(feature = "contract-spec")]
mod support;

use soroban_client::{
    contract_client::{Client, InvocationOptions},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    xdr::{ScSpecTypeDef, ScVal},
    Options, Server,
};

#[tokio::test]
#[ignore = "requires GENERIC_VAL_CONTRACT_ID from the deployed native fixture"]
async fn deployed_generic_val_round_trips_as_explicit_scval(
) -> Result<(), Box<dyn std::error::Error>> {
    let id = std::env::var("GENERIC_VAL_CONTRACT_ID")?;
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let key = Keypair::random()?;
    let source = server.request_airdrop(&key.public_key()).await?;
    let client = Client::new(&server, &id).await?;
    let function = client.spec().function("echo_val")?;
    assert_eq!(function.inputs[0].type_, ScSpecTypeDef::Val);
    assert_eq!(function.outputs.as_slice(), &[ScSpecTypeDef::Val]);

    let expected = support::generic_val_value();
    let call = client.call("echo_val", &(&expected,))?;
    let tx = call.transaction(&source, Networks::testnet(), &InvocationOptions::default())?;
    let simulation = server.simulate_transaction(&tx, None).await?;
    assert!(
        simulation.error.is_none(),
        "contract simulation failed: {:?}",
        simulation.error
    );
    let (raw, _) = simulation
        .try_result()?
        .ok_or("missing raw simulation result")?;
    assert_eq!(
        raw, expected,
        "the contract itself must echo the original wire value"
    );

    let actual: ScVal = call.simulate(&source, Networks::testnet()).await?;

    assert_eq!(actual, expected);
    Ok(())
}
