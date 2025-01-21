use crate::error::*;
use crate::server::*;
use crate::soroban_rpc::GetHealthResponse;
use crate::soroban_rpc::GetHealthWrapperResponse;
use crate::soroban_rpc::GetLatestLedgerResponse;
use crate::soroban_rpc::GetNetworkResponse;
use crate::soroban_rpc::GetNetworkResponseWrapper;
use crate::soroban_rpc::GetTransactionStatus;
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
use stellar_baselib::xdr::InvokeHostFunctionOp;
use stellar_baselib::xdr::InvokeHostFunctionResult;
use stellar_baselib::xdr::LedgerEntryData;
use stellar_baselib::xdr::LedgerKey;
use stellar_baselib::xdr::LedgerKeyAccount;
use stellar_baselib::xdr::LedgerKeyContractData;
use stellar_baselib::xdr::Limits;
use stellar_baselib::xdr::OperationResult;
use stellar_baselib::xdr::OperationResultTr;
use stellar_baselib::xdr::ScVal;
use stellar_baselib::xdr::ScVec;
use stellar_baselib::xdr::TransactionMeta;
use stellar_baselib::xdr::TransactionMetaV3;
use stellar_baselib::xdr::TransactionResult;
use stellar_baselib::xdr::TransactionResultResult;
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

#[tokio::test]
async fn get_transaction() {
    {
        let hash = "6bc97bddc21811c626839baf4ab574f4f9f7ddbebb44d286ae504396d4e752da";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "getTransaction",
          "params": {
            "hash": hash
          }
        }
                );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "status": "SUCCESS",
            "latestLedger": 2540076,
            "latestLedgerCloseTime": "1700086333",
            "oldestLedger": 2538637,
            "oldestLedgerCloseTime": "1700078796",
            "applicationOrder": 1,
            "envelopeXdr": "AAAAAgAAAADGFY14/R1KD0VGtTbi5Yp4d7LuMW0iQbLM/AUiGKj5owCpsoQAJY3OAAAjqgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABhhOwI+RL18Zpk7cqI5pRRf0L96jE8i+0x3ekhuBh2cUAAAARc2V0X2N1cnJlbmN5X3JhdGUAAAAAAAACAAAADwAAAANldXIAAAAACQAAAAAAAAAAAAAAAAARCz4AAAABAAAAAAAAAAAAAAABhhOwI+RL18Zpk7cqI5pRRf0L96jE8i+0x3ekhuBh2cUAAAARc2V0X2N1cnJlbmN5X3JhdGUAAAAAAAACAAAADwAAAANldXIAAAAACQAAAAAAAAAAAAAAAAARCz4AAAAAAAAAAQAAAAAAAAABAAAAB4408vVXuLU3mry897TfPpYjjsSN7n42REos241RddYdAAAAAQAAAAYAAAABhhOwI+RL18Zpk7cqI5pRRf0L96jE8i+0x3ekhuBh2cUAAAAUAAAAAQFvcYAAAImAAAAHxAAAAAAAAAACAAAAARio+aMAAABATbFMyom/TUz87wHex0LoYZA8jbNJkXbaDSgmOdk+wSBFJuMuta+/vSlro0e0vK2+1FqD/zWHZeYig4pKmM3rDA==",
            "resultXdr": "AAAAAAARFy8AAAAAAAAAAQAAAAAAAAAYAAAAAMu8SHUN67hTUJOz3q+IrH9M/4dCVXaljeK6x1Ss20YWAAAAAA==",
            "resultMetaXdr": "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",
            "ledger": 2540064,
            "createdAt": "1700086268"
          }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.get_transaction(hash).await;

        if let Ok(r) = txresult {
            assert_eq!(r.status, GetTransactionStatus::SUCCESS);
            assert_eq!(r.latestLedger, 2540076);
            assert_eq!(r.oldestLedger, 2538637);
            assert_eq!(r.applicationOrder, Some(1));
            let result = dbg!(r.getResult().unwrap());
            assert_eq!(result.fee_charged, 1120047);
            if let TransactionResultResult::TxSuccess(ops) = result.result {
                let op = ops.first().unwrap();

                if let OperationResult::OpInner(OperationResultTr::InvokeHostFunction(
                    InvokeHostFunctionResult::Success(Hash(h)),
                )) = op
                {
                    let mut expected_h = [0; 32];
                    hex::decode_to_slice(
                        "cbbc48750debb8535093b3deaf88ac7f4cff87425576a58de2bac754acdb4616",
                        &mut expected_h,
                    )
                    .expect("Cannot convert the expected result");
                    assert_eq!(h, &expected_h);
                } else {
                    panic!("InvokeHostFunctionResult not found")
                }
                //
            } else {
                panic!("TransactionResultResult not found")
            }
            let envelope = r.getEnvelope().expect("Should not fail");
            let (meta, val) = r.getResultMeta().expect("Should not fail");
            assert_eq!(val, Some(ScVal::Void));
            // TODO add more tests
        }
    }

    /*
     * Not found transaction
     */
    {
        let hash = "85f7aa8bfda425b98c0e53ffe56796ffd8865ec2fcc3ad71abf120801e2a14e5";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "getTransaction",
          "params": {
            "hash": hash
          }
        }
                );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "status": "NOT_FOUND",
            "latestLedger": 2540099,
            "latestLedgerCloseTime": "1700086455",
            "oldestLedger": 2538660,
            "oldestLedgerCloseTime": "1700078913"
          }
        }
                );

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.get_transaction(hash).await;
        if let Ok(r) = txresult {
            assert_eq!(r.status, GetTransactionStatus::NOT_FOUND);
            assert_eq!(r.latestLedger, 2540099);
            assert_eq!(r.oldestLedger, 2538660);
            assert_eq!(r.applicationOrder, None);
            assert_eq!(r.getResult(), None);
        }
    }
    /*
     * Failed transaction
     */
    {
        let hash = "2e4c699cbcb8ee83fffb857c9579bcc91f73f0df2a0444292f66e37563785929";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "getTransaction",
          "params": {
            "hash": hash
          }
        }
                );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 8675309,
          "result": {
            "status": "FAILED",
            "latestLedger": 2540124,
            "latestLedgerCloseTime": "1700086588",
            "oldestLedger": 2538685,
            "oldestLedgerCloseTime": "1700079044",
            "applicationOrder": 2,
            "envelopeXdr": "AAAAAgAAAABZvyflsZ5FumtSdS+t0/YnWWML3YWdzX1BGk/Qy786aQAAAG4AFyJfAABKvgAAAAIAAAAAAAAAAQAmwlEAJsK1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAE/fr1kk7lqP0srDPW4JExF1MWmpsG49FsRE3b0vMCmzAAAAAUVVUlMAAAAArPm4/+q9j4dl178h2JjrqjgRXnQ1WiGkyVy+nv3nGkEAAAAAAVjZQAAAAAAAAAABy786aQAAAEDybJBtG7V5NrRFpoboRUN/5ecys5wSUgag3CnTtWLmq3JDOxrEjK9noAnu/F5O0E8iXuVzX9BxZSO9JZ+Tw6kK",
            "resultXdr": "AAAAAAAAAGT/////AAAAAQAAAAAAAAAB////+gAAAAA=",
            "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAmwlIAAAAAAAAAAFm/J+WxnkW6a1J1L63T9idZYwvdhZ3NfUEaT9DLvzppAAAAF0g7NXsAFyJfAABKvQAAAAEAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAACbCSwAAAABlVULiAAAAAAAAAAEAJsJSAAAAAAAAAABZvyflsZ5FumtSdS+t0/YnWWML3YWdzX1BGk/Qy786aQAAABdIOzV7ABciXwAASr4AAAABAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAmwlIAAAAAZVVDBwAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
            "ledger": 2540114,
            "createdAt": "1700086535"
          }
        }
            );

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.get_transaction(hash).await;
        if let Ok(r) = txresult {
            assert_eq!(r.status, GetTransactionStatus::FAILED);
            assert_eq!(r.latestLedger, 2540124);
            assert_eq!(r.oldestLedger, 2538685);
            assert_eq!(r.applicationOrder, Some(2));
            let result = r.getResult().unwrap();
            if let TransactionResultResult::TxFailed(ops) = result.result {
                let op = ops.first().unwrap();
                assert_eq!(
                    op,
                    &OperationResult::OpInner(OperationResultTr::Payment(
                        stellar_baselib::xdr::PaymentResult::NoTrust
                    ))
                );
            } else {
                panic!("Expect a failed transaction")
            }
        }
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
