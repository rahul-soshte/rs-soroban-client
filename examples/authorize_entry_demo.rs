/// Soroban Auth Entry Signing Demo (CAP-0071, V2-by-default since Protocol 28)
///
/// Demonstrates `authorize_entry` from stellar-baselib, which mirrors the
/// behaviour of `authorizeEntry` in `js-stellar-base`.
///
/// Scenarios shown:
///   1. V1 signing  — `SOROBAN_CREDENTIALS_ADDRESS`  (legacy, opt out with `Some(false)`)
///   2. V2 signing  — `SOROBAN_CREDENTIALS_ADDRESS_V2` (Protocol 27 / CAP-0071-02,
///      address-bound to prevent replay across accounts sharing a key; this is the
///      DEFAULT since Protocol 28, matching js-stellar-sdk v17)
///   3. SourceAccount pass-through — no signing needed
///
/// This example runs fully offline; no network connection is required.
///
/// Run with:  cargo run --example authorize_entry_demo
use soroban_client::{
    authorize_entry::{authorize_entry, AuthorizeEntryParams},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    xdr::{
        AccountId, ContractId, Hash, InvokeContractArgs, PublicKey, ScAddress, ScSymbol, ScVal,
        SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
        SorobanAuthorizedInvocation, SorobanCredentials, StringM, Uint256, VecM,
    },
};
use std::str::FromStr;

fn main() {
    let keypair = Keypair::random().expect("keypair generation failed");
    let public_key = keypair.public_key();
    println!("Signing keypair: {}", public_key);

    // Build a ScAddress for this keypair's account.
    let account_address = ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(
        Uint256(keypair.raw_pubkey()),
    )));

    // Construct a realistic auth entry: calling `increment` on a mock contract.
    let contract_id = ContractId(Hash([0xab; 32]));
    let invocation = SorobanAuthorizedInvocation {
        function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
            contract_address: ScAddress::Contract(contract_id),
            function_name: ScSymbol(StringM::from_str("increment").unwrap()),
            args: VecM::default(),
        }),
        sub_invocations: VecM::default(),
    };

    // -------------------------------------------------------------------------
    // Scenario 1: V1 signing — SOROBAN_CREDENTIALS_ADDRESS
    // This is the legacy credential type, compatible with all Soroban protocols.
    // Since Protocol 28 the SDK defaults to V2, so V1 requires an explicit
    // opt-out with use_address_v2: Some(false) — equivalent to JS
    // authorizeEntry(..., { useV2: false }).
    // -------------------------------------------------------------------------
    println!("\n=== Scenario 1: V1 signing (SOROBAN_CREDENTIALS_ADDRESS) ===");

    let v1_entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address: account_address.clone(),
            nonce: 1_000_000,
            signature_expiration_ledger: 0,
            signature: ScVal::Void,
        }),
        root_invocation: invocation.clone(),
    };

    let signed_v1 = authorize_entry(AuthorizeEntryParams {
        entry: v1_entry,
        signer: &keypair,
        valid_until_ledger_seq: 5_000_000,
        network_passphrase: Networks::testnet(),
        use_address_v2: Some(false),
    })
    .expect("V1 signing failed");

    match &signed_v1.credentials {
        SorobanCredentials::Address(creds) => {
            println!("Credential type : SOROBAN_CREDENTIALS_ADDRESS (V1)");
            println!("Expiry ledger   : {}", creds.signature_expiration_ledger);
            println!(
                "Signature set   : {}",
                creds.signature != ScVal::Void
            );
        }
        other => panic!("Unexpected credential type: {:?}", other),
    }

    // -------------------------------------------------------------------------
    // Scenario 2: V2 signing — SOROBAN_CREDENTIALS_ADDRESS_V2 (CAP-0071-02)
    // The V2 preimage binds the auth hash to the specific account address,
    // preventing replay if two accounts share the same private key.
    // This is the default since Protocol 28: use_address_v2: None behaves the same.
    // -------------------------------------------------------------------------
    println!("\n=== Scenario 2: V2 signing (SOROBAN_CREDENTIALS_ADDRESS_V2) ===");

    let v2_entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address: account_address.clone(),
            nonce: 2_000_000,
            signature_expiration_ledger: 0,
            signature: ScVal::Void,
        }),
        root_invocation: invocation.clone(),
    };

    let signed_v2 = authorize_entry(AuthorizeEntryParams {
        entry: v2_entry,
        signer: &keypair,
        valid_until_ledger_seq: 5_000_001,
        network_passphrase: Networks::testnet(),
        use_address_v2: None, // None defaults to V2 (CAP-71 flip, Protocol 28)
    })
    .expect("V2 signing failed");

    match &signed_v2.credentials {
        SorobanCredentials::AddressV2(creds) => {
            println!("Credential type : SOROBAN_CREDENTIALS_ADDRESS_V2 (P27)");
            println!("Expiry ledger   : {}", creds.signature_expiration_ledger);
            println!(
                "Signature set   : {}",
                creds.signature != ScVal::Void
            );
        }
        other => panic!("Unexpected credential type: {:?}", other),
    }

    // -------------------------------------------------------------------------
    // Scenario 3: Incoming AddressV2 entry — stays V2 even when flag is false.
    // Once an entry is already V2, the flag cannot downgrade it.
    // -------------------------------------------------------------------------
    println!("\n=== Scenario 3: Incoming AddressV2 entry preserves its type ===");

    let already_v2_entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::AddressV2(SorobanAddressCredentials {
            address: account_address.clone(),
            nonce: 3_000_000,
            signature_expiration_ledger: 0,
            signature: ScVal::Void,
        }),
        root_invocation: invocation.clone(),
    };

    let signed_still_v2 = authorize_entry(AuthorizeEntryParams {
        entry: already_v2_entry,
        signer: &keypair,
        valid_until_ledger_seq: 5_000_002,
        network_passphrase: Networks::testnet(),
        use_address_v2: Some(false), // opt-out requested, but incoming type wins
    })
    .expect("Signing already-V2 entry failed");

    match &signed_still_v2.credentials {
        SorobanCredentials::AddressV2(creds) => {
            println!("Credential type : SOROBAN_CREDENTIALS_ADDRESS_V2 (preserved)");
            println!("Expiry ledger   : {}", creds.signature_expiration_ledger);
            println!(
                "Signature set   : {}",
                creds.signature != ScVal::Void
            );
        }
        other => panic!("Expected AddressV2 to be preserved, got: {:?}", other),
    }

    // -------------------------------------------------------------------------
    // Scenario 4: SourceAccount entry — passes through without signing.
    // The transaction's source account signature covers this; no key needed.
    // Equivalent to the JS SDK early-return for SOROBAN_CREDENTIALS_SOURCE_ACCOUNT.
    // -------------------------------------------------------------------------
    println!("\n=== Scenario 4: SourceAccount entry passes through unsigned ===");

    let source_entry = SorobanAuthorizationEntry {
        credentials: SorobanCredentials::SourceAccount,
        root_invocation: invocation.clone(),
    };

    let passed_through = authorize_entry(AuthorizeEntryParams {
        entry: source_entry,
        signer: &keypair,
        valid_until_ledger_seq: 9_999_999,
        network_passphrase: Networks::testnet(),
        use_address_v2: None,
    })
    .expect("SourceAccount pass-through failed");

    match &passed_through.credentials {
        SorobanCredentials::SourceAccount => {
            println!("Credential type : SOROBAN_CREDENTIALS_SOURCE_ACCOUNT");
            println!("No signing performed — covered by transaction signature.");
        }
        other => panic!("Expected SourceAccount, got: {:?}", other),
    }

    // -------------------------------------------------------------------------
    // Sanity check: V1 and V2 produce different payloads (different signing hash)
    // -------------------------------------------------------------------------
    println!("\n=== V1 vs V2 preimage difference ===");

    let sig_v1 = match &signed_v1.credentials {
        SorobanCredentials::Address(c) => extract_sig_bytes(&c.signature),
        _ => unreachable!(),
    };
    let sig_v2 = match &signed_v2.credentials {
        SorobanCredentials::AddressV2(c) => extract_sig_bytes(&c.signature),
        _ => unreachable!(),
    };

    assert_ne!(sig_v1, sig_v2, "V1 and V2 must produce different signatures");
    println!("V1 sig (first 8 bytes): {}", hex::encode(&sig_v1[..8]));
    println!("V2 sig (first 8 bytes): {}", hex::encode(&sig_v2[..8]));
    println!("Signatures differ      : yes (different preimage types)");

    println!("\nAll scenarios passed.");
}

/// Extract the 64-byte Ed25519 signature from the `ScvVec([ScvMap({..., signature: ScvBytes})])` shape.
fn extract_sig_bytes(sig_scval: &ScVal) -> Vec<u8> {
    if let ScVal::Vec(Some(vec)) = sig_scval {
        if let Some(ScVal::Map(Some(map))) = vec.first() {
            for entry in map.iter() {
                if let ScVal::Symbol(sym) = &entry.key {
                    if sym.0.as_slice() == b"signature" {
                        if let ScVal::Bytes(b) = &entry.val {
                            return b.0.to_vec();
                        }
                    }
                }
            }
        }
    }
    panic!("Could not extract signature bytes from ScVal: {:?}", sig_scval);
}
