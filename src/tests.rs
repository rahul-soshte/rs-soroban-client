use crate::error::*;
use crate::server::*;
use crate::soroban_rpc::GetHealthResponse;
use crate::soroban_rpc::GetHealthWrapperResponse;
use crate::soroban_rpc::GetLatestLedgerResponse;
use crate::soroban_rpc::GetNetworkResponse;
use crate::soroban_rpc::GetNetworkResponseWrapper;
use base64::Engine;
use serde_json::json;
use stellar_baselib::account::AccountBehavior;
use stellar_baselib::address::Address;
use stellar_baselib::address::AddressTrait;
use stellar_baselib::hashing;
use stellar_baselib::hashing::HashingBehavior;
use stellar_baselib::keypair::Keypair;
use stellar_baselib::keypair::KeypairBehavior;
use stellar_baselib::xdr::ContractDataEntry;
use stellar_baselib::xdr::ExtensionPoint;
use stellar_baselib::xdr::Hash;
use stellar_baselib::xdr::LedgerEntryData;
use stellar_baselib::xdr::LedgerKey;
use stellar_baselib::xdr::LedgerKeyAccount;
use stellar_baselib::xdr::LedgerKeyContractData;
use stellar_baselib::xdr::Limits;
use stellar_baselib::xdr::ScVal;
use stellar_baselib::xdr::ScVec;
use stellar_baselib::xdr::TtlEntry;
use stellar_baselib::xdr::WriteXdr;
use wiremock::matchers;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;

#[test]
fn server_new() {
    let s1 = Server::new(
        "https://rpc",
        Options {
            allow_http: None,
            timeout: None,
            headers: None,
        },
    );
    assert!(s1.is_ok(), "https scheme with allow_http None");

    let s2 = Server::new(
        "/rpc",
        Options {
            allow_http: None,
            timeout: None,
            headers: None,
        },
    );
    assert!(matches!(
        s2.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme)),
    ));

    let s3 = Server::new(
        "/rpc",
        Options {
            allow_http: Some(true),
            timeout: None,
            headers: None,
        },
    );
    assert!(matches!(
        s3.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme)),
    ));

    let s4 = Server::new(
        "http://rpc",
        Options {
            allow_http: Some(true),
            timeout: None,
            headers: None,
        },
    );
    assert!(s4.is_ok(), "http scheme with allow_http true");

    let s5 = Server::new(
        "",
        Options {
            allow_http: Some(true),
            timeout: None,
            headers: None,
        },
    );
    assert!(matches!(
        s5.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::InvalidUri(_))),
    ));

    let s6 = Server::new(
        "http://rpc",
        Options {
            allow_http: Some(false),
            timeout: None,
            headers: None,
        },
    );
    assert!(matches!(
        s6.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::UnsecureHttpNotAllowed)),
    ));
}

#[tokio::test]
async fn get_health() {
    let request = json!({"method": "getHealth"});
    let response = json!({"jsonrpc": "2.0", "id": 1, "result": {"status": "healthy"}});
    let (s, _m) = get_mocked_server(request, response).await;
    let result = s.get_health().await.expect("Should not fail");

    let expect = GetHealthWrapperResponse {
        jsonrpc: "2.0".to_string(),
        id: 1,
        result: GetHealthResponse {
            status: "healthy".to_string(),
        },
    };

    assert_eq!(dbg!(result), expect);
}

#[tokio::test]
async fn get_latest_ledger() {
    let request = json!({"method": "getLatestLedger"});
    let response = json!(
        {
            "jsonrpc": "2.0",
            "id": 8675309,
            "result": {
            "id": "c73c5eac58a441d4eb733c35253ae85f783e018f7be5ef974258fed067aabb36",
            "protocolVersion": 20,
            "sequence": 2539605
        }
    }
    );

    let (s, _m) = get_mocked_server(request, response).await;
    let result = s.get_latest_ledger().await.expect("Should not fail");
    let expect = GetLatestLedgerResponse {
        id: "c73c5eac58a441d4eb733c35253ae85f783e018f7be5ef974258fed067aabb36".into(),
        sequence: 2539605,
        protocol_version: 20,
    };
    assert_eq!(dbg!(result), expect);
}

#[tokio::test]
async fn get_network() {
    let request = json!({"method": "getNetwork"});
    let response = json!(
        {
            "jsonrpc": "2.0",
            "id": 8675309,
            "result": {
            "friendbotUrl": "https://friendbot-testnet.stellar.org/",
            "passphrase": "Test SDF Network ; September 2015",
            "protocolVersion": 20
        }
    }
    );

    let (s, _m) = get_mocked_server(request, response).await;
    let result = s.get_network().await.expect("Should not fail");
    let expect = GetNetworkResponseWrapper {
        jsonrpc: "2.0".into(),
        id: 8675309,
        result: GetNetworkResponse {
            friendbotUrl: Some("https://friendbot-testnet.stellar.org/".into()),
            passphrase: Some("Test SDF Network ; September 2015".into()),
            protocolVersion: Some(20),
        },
    };
    assert_eq!(dbg!(result), expect);
}

#[tokio::test]
async fn get_account() {
    let address = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";
    let account_id = Keypair::from_public_key(address)
        .expect("Should not fail")
        .xdr_account_id();
    let key = LedgerKey::Account(LedgerKeyAccount { account_id });
    let account_entry = "AAAAAAAAAABzdv3ojkzWHMD7KUoXhrPx0GH18vHKV0ZfqpMiEblG1g3gtpoE608YAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAAAAQAAAAAY9D8iA";

    let value = base64::prelude::BASE64_STANDARD.encode(key.to_xdr(Limits::none()).unwrap());
    let request = json!(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": { "keys": [value] },
    }
    );
    let response = json!(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
            "entries": [
            {
                "key": value,
                "xdr": account_entry,
                "lastModifiedLedgerSeq": 2552504
            }
        ],
            "latestLedger": 2552990
        }
    }
    );

    let (s, _m) = get_mocked_server(request, response).await;
    let result = s.get_account(address).await.expect("Should not fail");
    assert_eq!(result.sequence_number(), "1");
    assert_eq!(result.account_id(), address);
}

#[tokio::test]
async fn get_account_not_found() {
    let address = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";
    let account_id = Keypair::from_public_key(address)
        .expect("Should not fail")
        .xdr_account_id();
    let key = LedgerKey::Account(LedgerKeyAccount { account_id });

    let value = base64::prelude::BASE64_STANDARD.encode(key.to_xdr(Limits::none()).unwrap());
    let request = json!(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": { "keys": [value] },
    }
    );
    let response = json!(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
            "entries": null,
            "latestLedger": 2552990
        }
    }
    );

    let (s, _m) = get_mocked_server(request, response).await;
    let result = s.get_account(address).await;
    assert!(matches!(result, Err(Error::AccountNotFound)));
}

#[tokio::test]
async fn get_ledger_entries() {
    let address = "CCJZ5DGASBWQXR5MPFCJXMBI333XE5U3FSJTNQU7RIKE3P5GN2K2WYD5";
    let key = ScVal::Vec(Some(ScVec(
        [ScVal::Symbol("test".try_into().unwrap())]
            .try_into()
            .unwrap(),
    )));
    let contract = Address::new(address).unwrap().to_sc_address().unwrap();
    let durability = stellar_baselib::xdr::ContractDataDurability::Persistent;
    let ledger_entry = LedgerEntryData::ContractData(ContractDataEntry {
        ext: ExtensionPoint::V0,
        contract: contract.clone(),
        durability,
        key: key.clone(),
        val: key.clone(),
    });
    let ledger_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract,
        key: key.clone(),
        durability,
    });
    let h = hashing::Sha256Hasher::hash(ledger_key.to_xdr(Limits::none()).unwrap());
    let ledger_ttl_entry = TtlEntry {
        key_hash: Hash(h),
        live_until_ledger_seq: 1000,
    };
    let ledger_key_xdr = ledger_key.to_xdr_base64(Limits::none()).unwrap();
    let ledger_entry_xdr = ledger_entry.to_xdr_base64(Limits::none()).unwrap();

    /*
     * ledger entry found, includes ttl meta in response
     */
    {
        let request = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLedgerEntries",
                "params": { "keys": [ledger_key_xdr] },
        }
        );
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                "entries": [
            {
              "liveUntilLedgerSeq": ledger_ttl_entry.live_until_ledger_seq,
              "lastModifiedLedgerSeq": 2,
              "key": ledger_key_xdr,
              "xdr": ledger_entry_xdr,
            },
          ],
                "latestLedger": 2552990
            }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let result = s
            .get_ledger_entries(vec![ledger_key.clone()])
            .await
            .expect("Should not fail");
        if let Some(entries) = result.result.entries {
            assert_eq!(entries.len(), 1);
            let e = &entries[0];
            assert_eq!(e.last_modified_ledger_seq, Some(2));
            assert_eq!(e.key, ledger_key_xdr);
            assert_eq!(e.xdr, ledger_entry_xdr);
            assert_eq!(
                e.live_until_ledger_seq,
                Some(ledger_ttl_entry.live_until_ledger_seq)
            );
        } else {
            panic!("No entry found");
        }
    }

    /*
     * ledger entry found, no ttl in response
     */
    {
        let request = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLedgerEntries",
                "params": { "keys": [ledger_key_xdr] },
        }
        );
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                "entries": [
            {
              "lastModifiedLedgerSeq": 2,
              "key": ledger_key_xdr,
              "xdr": ledger_entry_xdr,
            },
          ],
                "latestLedger": 2552990
            }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let result = s
            .get_ledger_entries(vec![ledger_key.clone()])
            .await
            .expect("Should not fail");
        if let Some(entries) = result.result.entries {
            assert_eq!(entries.len(), 1);
            let e = &entries[0];
            assert_eq!(e.last_modified_ledger_seq, Some(2));
            assert_eq!(e.key, ledger_key_xdr);
            assert_eq!(e.xdr, ledger_entry_xdr);
            assert_eq!(e.live_until_ledger_seq, None);
        } else {
            panic!("No entry found");
        }
        //
    }

    /*
     * throws when invalid rpc response
     */
    {
        let request = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLedgerEntries",
                "params": { "keys": [ledger_key_xdr] },
        }
        );
        let response = json!(
        {
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
        "entries": [
            {
                 "lastModifiedLedgerSeq": 2,
            },
            {
                "lastModifiedLedgerSeq": 1,
            },
        ],
        "latestLedger": 2552990
        }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let result = s.get_ledger_entries(vec![ledger_key.clone()]).await;

        // TODO better error should be used
        assert!(matches!(result, Err(Error::NetworkError)));
    }
}

#[tokio::test]
async fn get_contract_data() {
    let address = "CCJZ5DGASBWQXR5MPFCJXMBI333XE5U3FSJTNQU7RIKE3P5GN2K2WYD5";
    let key = ScVal::Vec(Some(ScVec(
        [ScVal::Symbol("Admin".try_into().unwrap())]
            .try_into()
            .unwrap(),
    )));
    let contract = Address::new(address).unwrap().to_sc_address().unwrap();
    let durability = stellar_baselib::xdr::ContractDataDurability::Persistent;
    let ledger_entry = LedgerEntryData::ContractData(ContractDataEntry {
        ext: ExtensionPoint::V0,
        contract: contract.clone(),
        durability,
        key: key.clone(),
        val: key.clone(),
    });
    let ledger_key = LedgerKey::ContractData(LedgerKeyContractData {
        contract,
        key: key.clone(),
        durability,
    });
    let h = hashing::Sha256Hasher::hash(ledger_key.to_xdr(Limits::none()).unwrap());
    let ledger_ttl_entry = TtlEntry {
        key_hash: Hash(h),
        live_until_ledger_seq: 1000,
    };
    let ledger_key_xdr = ledger_key.to_xdr_base64(Limits::none()).unwrap();
    let ledger_entry_xdr = ledger_entry.to_xdr_base64(Limits::none()).unwrap();

    /*
     * contract data found
     */
    {
        let request = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLedgerEntries",
                "params": { "keys": [ledger_key_xdr] },
        }
        );
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                "entries": [
                {
                    "liveUntilLedgerSeq": ledger_ttl_entry.live_until_ledger_seq,
                    "lastModifiedLedgerSeq": 2,
                    "key": ledger_key_xdr,
                    "xdr": ledger_entry_xdr,
                },
            ],
            "latestLedger": 2552990
        }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let result = s
            .get_contract_data(address, key.clone(), Durability::Persistent)
            .await
            .expect("Should not fail");

        assert_eq!(result.key, ledger_key_xdr);
        assert_eq!(result.xdr, ledger_entry_xdr);
        assert_eq!(
            result.live_until_ledger_seq,
            Some(ledger_ttl_entry.live_until_ledger_seq)
        );
    }
    /*
     * contract data not found
     */
    {
        let request = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLedgerEntries",
                "params": { "keys": [ledger_key_xdr] },
        }
        );
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                "entries": [],
            "latestLedger": 2552990
        }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let result = s
            .get_contract_data(address, key.clone(), Durability::Persistent)
            .await;

        assert!(matches!(result, Err(Error::ContractDataNotFound)));
    }
}

// Create a Server that will reply `response` for a json `request` partially matching
async fn get_mocked_server(
    request: serde_json::Value,
    response: serde_json::Value,
) -> (Server, MockServer) {
    let mock_server = MockServer::start().await;
    let server_url = mock_server.uri();

    let response = ResponseTemplate::new(200).set_body_json(response);
    Mock::given(method("POST"))
        .and(path("/"))
        .and(matchers::body_partial_json(request))
        .respond_with(response)
        .expect(1..)
        .mount(&mock_server)
        .await;

    let server = Server::new(
        &server_url,
        Options {
            allow_http: Some(true),
            timeout: None,
            headers: None,
        },
    )
    .expect("Configuration should not fail");

    (server, mock_server)
}
