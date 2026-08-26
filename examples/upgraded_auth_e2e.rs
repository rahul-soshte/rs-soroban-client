/// End-to-end check of the `use_upgraded_auth` simulation flag against testnet.
///
/// Two accounts are funded: `A` is the transaction source, `B` is a second
/// account whose authorization is required by the invocation. Calling
/// `transfer(from = B, to = A, ...)` on the native SAC makes recording-mode
/// simulation hand back an auth entry for `B`, which is what the flag changes:
///
///   1. simulate with `use_upgraded_auth: Some(false)` -> SOROBAN_CREDENTIALS_ADDRESS
///   2. simulate with `use_upgraded_auth: Some(true)`  -> SOROBAN_CREDENTIALS_ADDRESS_V2
///   3. sign the v2 entry with `authorize_entry` (address-bound preimage)
///   4. re-simulate in enforce mode, which makes the host verify that signature
///   5. submit and wait for the transaction to succeed on-chain
///
/// Steps 4 and 5 are the real test: an entry signed with the wrong preimage is
/// accepted locally and rejected by the host, so only a live run proves it out.
///
/// Run with:  cargo run --example upgraded_auth_e2e
use std::time::Duration;

use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    authorize_entry::{authorize_entry, AuthorizeEntryParams},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    soroban_rpc::TransactionStatus,
    transaction::{
        assemble_transaction, TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior,
    },
    xdr::{Int128Parts, OperationBody, ScVal, SorobanAuthorizationEntry, SorobanCredentials},
    AuthMode, Options, Server, SimulationOptions,
};

/// Native XLM Stellar Asset Contract on testnet.
const NATIVE_SAC: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

fn credential_name(entry: &SorobanAuthorizationEntry) -> &'static str {
    match entry.credentials {
        SorobanCredentials::SourceAccount => "SOURCE_ACCOUNT",
        SorobanCredentials::Address(_) => "ADDRESS (v1)",
        SorobanCredentials::AddressV2(_) => "ADDRESS_V2",
        SorobanCredentials::AddressWithDelegates(_) => "ADDRESS_WITH_DELEGATES",
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let network = Networks::testnet();

    let version = server.get_version_info().await?;
    println!("RPC version    : {}", version.version);
    println!("Protocol       : {}", version.protocol_version);
    println!();

    // -----------------------------------------------------------------------
    // Fund the source account (A) and the authorizing account (B).
    // -----------------------------------------------------------------------
    let kp_a = Keypair::random()?;
    let kp_b = Keypair::random()?;
    println!("Source    (A)  : {}", kp_a.public_key());
    println!("Authorizer(B)  : {}", kp_b.public_key());

    let a_data = server.request_airdrop(&kp_a.public_key()).await?;
    server.request_airdrop(&kp_b.public_key()).await?;
    let mut account_a = Account::new(&kp_a.public_key(), &a_data.sequence_number())?;
    println!("Both accounts funded.");
    println!();

    // -----------------------------------------------------------------------
    // transfer(from = B, to = A, 1 stroop). `from.require_auth()` inside the
    // SAC is what makes simulation produce an address credential for B.
    // -----------------------------------------------------------------------
    let sac = Contracts::new(NATIVE_SAC)?;
    let op = sac.call(
        "transfer",
        Some(vec![
            Address::new(&kp_b.public_key())?.to_sc_val()?,
            Address::new(&kp_a.public_key())?.to_sc_val()?,
            ScVal::I128(Int128Parts { hi: 0, lo: 1 }),
        ]),
    );
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(op)
        .build();

    // -----------------------------------------------------------------------
    // 1. Legacy simulation: flag off.
    // -----------------------------------------------------------------------
    let sim_v1 = server
        .simulate_transaction(
            &tx,
            Some(SimulationOptions {
                auth_mode: Some(AuthMode::Record),
                use_upgraded_auth: Some(false),
                ..Default::default()
            }),
        )
        .await?;
    if let Some(e) = &sim_v1.error {
        return Err(format!("legacy simulation failed: {e}").into());
    }
    let (_, auth_v1) = sim_v1.to_result().ok_or("no simulation result")?;
    println!("=== use_upgraded_auth: false ===");
    for entry in &auth_v1 {
        println!("  credential   : {}", credential_name(entry));
    }
    assert!(
        auth_v1
            .iter()
            .all(|e| matches!(e.credentials, SorobanCredentials::Address(_))),
        "expected legacy ADDRESS credentials with the flag off"
    );
    println!();

    // -----------------------------------------------------------------------
    // 2. Upgraded simulation: flag on.
    // -----------------------------------------------------------------------
    let sim_v2 = server
        .simulate_transaction(
            &tx,
            Some(SimulationOptions {
                auth_mode: Some(AuthMode::Record),
                use_upgraded_auth: Some(true),
                ..Default::default()
            }),
        )
        .await?;
    if let Some(e) = &sim_v2.error {
        return Err(format!("upgraded simulation failed: {e}").into());
    }
    let (_, auth_v2) = sim_v2.to_result().ok_or("no simulation result")?;
    println!("=== use_upgraded_auth: true ===");
    for entry in &auth_v2 {
        println!("  credential   : {}", credential_name(entry));
    }
    assert!(
        auth_v2
            .iter()
            .any(|e| matches!(e.credentials, SorobanCredentials::AddressV2(_))),
        "expected ADDRESS_V2 credentials with the flag on"
    );
    println!();

    // -----------------------------------------------------------------------
    // 3. Sign the v2 entries with B. `use_address_v2: false` here: the entry is
    //    already v2, so authorize_entry must keep the arm and pick the
    //    address-bound preimage on its own.
    // -----------------------------------------------------------------------
    let valid_until = server.get_latest_ledger().await?.sequence + 100;
    let signed: Vec<SorobanAuthorizationEntry> = auth_v2
        .into_iter()
        .map(|entry| {
            authorize_entry(AuthorizeEntryParams {
                entry,
                signer: &kp_b,
                valid_until_ledger_seq: valid_until,
                network_passphrase: network,
                use_address_v2: false,
            })
        })
        .collect::<Result<_, _>>()?;
    println!("=== after authorize_entry (signer B) ===");
    for entry in &signed {
        println!("  credential   : {}", credential_name(entry));
    }
    assert!(
        signed
            .iter()
            .any(|e| matches!(e.credentials, SorobanCredentials::AddressV2(_))),
        "signing must not change the credential arm"
    );
    println!("  valid until  : ledger {valid_until}");
    println!();

    // -----------------------------------------------------------------------
    // 4. Enforce-mode simulation. This runs the auth framework, so a signature
    //    built from the wrong preimage fails right here.
    // -----------------------------------------------------------------------
    let host_function = match &tx.operations.as_ref().unwrap()[0].body {
        OperationBody::InvokeHostFunction(op) => op.host_function.clone(),
        _ => return Err("not an InvokeHostFunction operation".into()),
    };
    let mut tx_with_auth = tx.clone();
    tx_with_auth.operations = Some(vec![Operation::new()
        .invoke_host_function(host_function, Some(signed))
        .map_err(|e| format!("cannot rebuild the operation: {e:?}"))?]);

    let sim_enforce = server
        .simulate_transaction(
            &tx_with_auth,
            Some(SimulationOptions {
                auth_mode: Some(AuthMode::Enforce),
                ..Default::default()
            }),
        )
        .await?;
    if let Some(e) = &sim_enforce.error {
        return Err(format!("enforce-mode simulation rejected the v2 signature: {e}").into());
    }
    println!("=== enforce-mode simulation ===");
    println!("  host accepted the ADDRESS_V2 signature");
    println!();

    // -----------------------------------------------------------------------
    // 5. Assemble, sign as A, submit.
    // -----------------------------------------------------------------------
    let mut ready = assemble_transaction(&tx_with_auth, sim_enforce)?;
    ready.sign(&[kp_a]);

    let response = server.send_transaction(ready).await?;
    println!("=== submission ===");
    println!("  tx hash      : {}", response.hash);

    match server
        .wait_transaction(&response.hash, Duration::from_secs(60))
        .await
    {
        Ok(result) if result.status == TransactionStatus::Success => {
            println!("  status       : SUCCESS");
            println!();
            println!("End-to-end v2 auth verified on testnet.");
            Ok(())
        }
        Ok(result) => {
            println!("  status       : {:?}", result.status);
            Err("transaction did not succeed".into())
        }
        Err((e, _)) => Err(format!("wait_transaction failed: {e:?}").into()),
    }
}
