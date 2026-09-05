/// End-to-end check of the Protocol 28 SDK changes against testnet.
///
/// Covers, in order:
///   1. CAP-71 default flip — `simulate_transaction` with no options returns
///      `SOROBAN_CREDENTIALS_ADDRESS_V2` entries (the SDK sends
///      `useUpgradedAuth: true` by default).
///   2. Wasm fetchers — upload + deploy a contract, then read the wasm back
///      with `get_contract_wasm_by_hash` and `get_contract_wasm_by_contract_id`.
///   3. CAP-85 — deploy an owner contract that publishes an executable
///      reference tag (`env.executable_refs().set(...)`), resolve it with
///      `get_external_ref_wasm_hash`, deploy a new contract from the external
///      reference with `create_contract_from_external_ref`, fetch its wasm
///      through the reference, and invoke it to prove the referenced code runs.
///
/// The owner contract's source lives in `examples/executable_owner_contract/`
/// (a standalone crate, see its README to rebuild the committed wasm).
///
/// Run with:  cargo run --example p28_e2e
use std::time::Duration;

use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    soroban_rpc::{GetTransactionResponse, SendTransactionStatus, TransactionStatus},
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr::{ContractExecutableExternalRef, Int128Parts, ScString, ScVal, SorobanCredentials},
    Options, Server,
};

/// Native XLM Stellar Asset Contract on testnet.
const NATIVE_SAC: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const TAG: &[u8] = b"p28-e2e";

/// Prepare, sign and submit `tx`, wait for it, and return the final result.
async fn submit(
    server: &Server,
    tx: &soroban_client::transaction::Transaction,
    signer: &Keypair,
) -> Result<GetTransactionResponse, Box<dyn std::error::Error>> {
    let mut ptx = server.prepare_transaction(tx).await?;
    ptx.sign(std::slice::from_ref(signer));
    let response = server.send_transaction(ptx).await?;
    if response.status == SendTransactionStatus::Error {
        return Err(format!(
            "submission rejected: {:?}",
            response.to_error_result()
        )
        .into());
    }
    match server
        .wait_transaction(&response.hash, Duration::from_secs(60))
        .await
    {
        Ok(result) if result.status == TransactionStatus::Success => Ok(result),
        Ok(result) => Err(format!("tx {} ended as {:?}", response.hash, result.status).into()),
        Err((e, _)) => Err(format!("wait_transaction failed: {e:?}").into()),
    }
}

/// Extract the ScVal return value of a successful transaction.
fn return_value(result: &GetTransactionResponse) -> Result<ScVal, Box<dyn std::error::Error>> {
    let (_meta, ret_val) = result.to_result_meta().ok_or("no result meta")?;
    ret_val.ok_or_else(|| "no return value".into())
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::new("https://soroban-testnet.stellar.org", Options::default())?;
    let network = Networks::testnet();

    let version = server.get_version_info().await?;
    println!("RPC version    : {}", version.version);
    println!("Protocol       : {}", version.protocol_version);
    assert!(
        version.protocol_version >= 28,
        "this end-to-end run needs a Protocol 28 network"
    );
    println!();

    let kp_a = Keypair::random()?;
    let kp_b = Keypair::random()?;
    println!("Source    (A)  : {}", kp_a.public_key());
    println!("Authorizer(B)  : {}", kp_b.public_key());
    let a_data = server.request_airdrop(&kp_a.public_key()).await?;
    server.request_airdrop(&kp_b.public_key()).await?;
    let mut account_a = Account::new(&kp_a.public_key(), &a_data.sequence_number())?;
    println!("Both accounts funded.");
    println!();

    // =======================================================================
    // 1. CAP-71 default flip: simulate with NO options at all must come back
    //    with ADDRESS_V2 credentials because the SDK now sends
    //    `useUpgradedAuth: true` by default.
    // =======================================================================
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

    let sim = server.simulate_transaction(&tx, None).await?;
    if let Some(e) = &sim.error {
        return Err(format!("default simulation failed: {e}").into());
    }
    let (_, auth) = sim.to_result().ok_or("no simulation result")?;
    assert!(
        auth.iter()
            .any(|e| matches!(e.credentials, SorobanCredentials::AddressV2(_))),
        "default simulation must return ADDRESS_V2 credentials on Protocol 28"
    );
    println!("[1] CAP-71 default flip: default simulation returned ADDRESS_V2  OK");
    println!();

    // Building the simulation-only transaction above consumed a local sequence
    // number that was never submitted; resync the account from the ledger.
    let mut account_a = server.get_account(&kp_a.public_key()).await?;

    // =======================================================================
    // 2. Wasm fetchers: upload + deploy, then read the wasm back both ways.
    // =======================================================================
    let auth_wasm = std::fs::read("./examples/soroban_auth_contract.wasm")?;

    let upload = Operation::new()
        .upload_wasm(&auth_wasm, None)
        .map_err(|e| format!("upload_wasm: {e:?}"))?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(upload)
        .build();
    let result = submit(&server, &tx, &kp_a).await?;
    let auth_wasm_hash: [u8; 32] = {
        let bytes: Vec<u8> = return_value(&result)?
            .try_into()
            .map_err(|_| "upload return value is not bytes")?;
        *bytes.last_chunk::<32>().ok_or("hash is not 32 bytes")?
    };
    println!("[2] uploaded wasm, hash {}", hex::encode(auth_wasm_hash));

    let fetched = server.get_contract_wasm_by_hash(auth_wasm_hash).await?;
    assert_eq!(fetched, auth_wasm, "wasm by hash differs from the upload");
    println!("[2] get_contract_wasm_by_hash matches the upload  OK");

    let create = Operation::new().create_contract(
        &kp_a.public_key(),
        auth_wasm_hash,
        None,
        None,
        [].into(),
    )
    .map_err(|e| format!("create operation: {e:?}"))?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(create)
        .build();
    let result = submit(&server, &tx, &kp_a).await?;
    let wasm_contract_id = match return_value(&result)? {
        ScVal::Address(addr) => Address::from_sc_address(&addr)?.to_string(),
        other => return Err(format!("unexpected create return: {other:?}").into()),
    };
    println!("[2] deployed contract {wasm_contract_id}");

    let fetched = server
        .get_contract_wasm_by_contract_id(&wasm_contract_id)
        .await?;
    assert_eq!(fetched, auth_wasm, "wasm by contract id differs");
    println!("[2] get_contract_wasm_by_contract_id matches the upload  OK");
    println!();

    // =======================================================================
    // 3. CAP-85: publish an executable reference tag and deploy from it.
    // =======================================================================
    // 3a. Upload + deploy the owner contract.
    let owner_wasm = std::fs::read("./examples/executable_owner_contract.wasm")?;
    let upload = Operation::new()
        .upload_wasm(&owner_wasm, None)
        .map_err(|e| format!("upload_wasm: {e:?}"))?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(upload)
        .build();
    let result = submit(&server, &tx, &kp_a).await?;
    let owner_wasm_hash: [u8; 32] = {
        let bytes: Vec<u8> = return_value(&result)?
            .try_into()
            .map_err(|_| "upload return value is not bytes")?;
        *bytes.last_chunk::<32>().ok_or("hash is not 32 bytes")?
    };
    let create = Operation::new().create_contract(
        &kp_a.public_key(),
        owner_wasm_hash,
        None,
        None,
        [].into(),
    )
    .map_err(|e| format!("create operation: {e:?}"))?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(create)
        .build();
    let result = submit(&server, &tx, &kp_a).await?;
    let owner_id = match return_value(&result)? {
        ScVal::Address(addr) => Address::from_sc_address(&addr)?.to_string(),
        other => return Err(format!("unexpected create return: {other:?}").into()),
    };
    println!("[3] deployed executable owner contract {owner_id}");

    // 3b. publish(tag, wasm_hash): the owner creates the ExecutableTag entry
    //     pointing at the auth contract wasm.
    let owner = Contracts::new(&owner_id)?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(owner.call(
            "publish",
            Some(vec![
                ScVal::String(ScString(TAG.to_vec().try_into().unwrap())),
                ScVal::Bytes(auth_wasm_hash.to_vec().try_into().unwrap()),
            ]),
        ))
        .build();
    submit(&server, &tx, &kp_a).await?;
    println!("[3] published executable tag {:?}", String::from_utf8_lossy(TAG));

    // 3c. Resolve the reference off-chain with the new RPC helper.
    let ext_ref = ContractExecutableExternalRef {
        executable_owner: Address::new(&owner_id)?.to_sc_address()?,
        tag: ScString(TAG.to_vec().try_into().unwrap()),
    };
    let resolved = server.get_external_ref_wasm_hash(ext_ref).await?;
    assert_eq!(
        resolved, auth_wasm_hash,
        "resolved wasm hash differs from the published one"
    );
    println!("[3] get_external_ref_wasm_hash resolved to the published hash  OK");

    // 3d. Deploy a new contract whose executable IS the external reference.
    let create = Operation::new().create_contract_from_external_ref(
        &kp_a.public_key(),
        &owner_id,
        TAG,
        None,
        None,
        [].into(),
    )
    .map_err(|e| format!("create operation: {e:?}"))?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(create)
        .build();
    let result = submit(&server, &tx, &kp_a).await?;
    let ext_contract_id = match return_value(&result)? {
        ScVal::Address(addr) => Address::from_sc_address(&addr)?.to_string(),
        other => return Err(format!("unexpected create return: {other:?}").into()),
    };
    println!("[3] deployed contract {ext_contract_id} from the external reference");

    // 3e. Fetching its wasm must resolve through the reference transparently.
    let fetched = server
        .get_contract_wasm_by_contract_id(&ext_contract_id)
        .await?;
    assert_eq!(
        fetched, auth_wasm,
        "wasm fetched through the external ref differs"
    );
    println!("[3] get_contract_wasm_by_contract_id resolved the external ref  OK");

    // 3f. Invoke the new contract: the referenced code must actually run.
    let ext_contract = Contracts::new(&ext_contract_id)?;
    let tx = TransactionBuilder::new(&mut account_a, network, None)
        .fee(1000u32)
        .add_operation(ext_contract.call(
            "increment",
            Some(vec![
                Address::account(kp_a.raw_public_key())?.to_sc_val()?,
                3u32.into(),
            ]),
        ))
        .build();
    let result = submit(&server, &tx, &kp_a).await?;
    let counter = return_value(&result)?;
    println!("[3] invoked increment on the external-ref contract, returned {counter:?}  OK");
    println!();

    println!("All Protocol 28 end-to-end checks passed on testnet.");
    Ok(())
}
