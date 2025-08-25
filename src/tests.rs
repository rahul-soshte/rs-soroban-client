use std::cell::RefCell;
use std::rc::Rc;
use std::vec;

use crate::error::*;
use crate::server::*;
use crate::soroban_rpc::GetHealthResponse;
use crate::soroban_rpc::GetLatestLedgerResponse;
use crate::soroban_rpc::GetNetworkResponse;
use crate::soroban_rpc::SendTransactionStatus;
use crate::soroban_rpc::StateChangeKind;
use crate::soroban_rpc::TransactionStatus;
use crate::EventFilter;
use crate::Topic;
use base64::Engine;
use serde_json::json;
use stellar_baselib::account::Account;
use stellar_baselib::account::AccountBehavior;
use stellar_baselib::address::Address;
use stellar_baselib::address::AddressTrait;
use stellar_baselib::contract::ContractBehavior;
use stellar_baselib::contract::Contracts;
use stellar_baselib::hashing;
use stellar_baselib::hashing::HashingBehavior;
use stellar_baselib::keypair::Keypair;
use stellar_baselib::keypair::KeypairBehavior;
use stellar_baselib::network::NetworkPassphrase;
use stellar_baselib::network::Networks;
use stellar_baselib::transaction::Transaction;
use stellar_baselib::transaction::TransactionBehavior;
use stellar_baselib::transaction_builder::TransactionBuilder;
use stellar_baselib::transaction_builder::TransactionBuilderBehavior;
use stellar_baselib::xdr::int128_helpers::i128_from_pieces;
use stellar_baselib::xdr::ContractDataEntry;
use stellar_baselib::xdr::ContractEvent;
use stellar_baselib::xdr::ContractEventBody;
use stellar_baselib::xdr::ContractEventType;
use stellar_baselib::xdr::ContractEventV0;
use stellar_baselib::xdr::ContractId;
use stellar_baselib::xdr::ExtensionPoint;
use stellar_baselib::xdr::GeneralizedTransactionSet;
use stellar_baselib::xdr::Hash;
use stellar_baselib::xdr::Int128Parts;
use stellar_baselib::xdr::InvokeHostFunctionResult;
use stellar_baselib::xdr::LedgerCloseMeta;
use stellar_baselib::xdr::LedgerCloseMetaV1;
use stellar_baselib::xdr::LedgerEntryData;
use stellar_baselib::xdr::LedgerHeaderHistoryEntry;
use stellar_baselib::xdr::LedgerKey;
use stellar_baselib::xdr::LedgerKeyAccount;
use stellar_baselib::xdr::LedgerKeyContractData;
use stellar_baselib::xdr::Limits;
use stellar_baselib::xdr::OperationResult;
use stellar_baselib::xdr::OperationResultTr;
use stellar_baselib::xdr::ScString;
use stellar_baselib::xdr::ScSymbol;
use stellar_baselib::xdr::ScVal;
use stellar_baselib::xdr::ScVec;
use stellar_baselib::xdr::SorobanResources;
use stellar_baselib::xdr::SorobanTransactionData;
use stellar_baselib::xdr::TimeBounds;
use stellar_baselib::xdr::TimePoint;
use stellar_baselib::xdr::TransactionEvent;
use stellar_baselib::xdr::TransactionEventStage;

use stellar_baselib::xdr::TransactionResult;
use stellar_baselib::xdr::TransactionResultResult;
use stellar_baselib::xdr::TransactionSetV1;
use stellar_baselib::xdr::TransactionV1Envelope;
use stellar_baselib::xdr::TtlEntry;
use stellar_baselib::xdr::WriteXdr;
use wiremock::matchers;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;

#[test]
fn server_new() {
    let s1 = Server::new("https://rpc", Default::default());
    assert!(s1.is_ok(), "https scheme with allow_http None");

    let s2 = Server::new("file://rpc", Default::default());
    assert!(matches!(
        s2.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme)),
    ));

    let s3 = Server::new(
        "scheme://rpc",
        Options {
            allow_http: true,
            ..Default::default()
        },
    );
    assert!(matches!(
        s3.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::NotHttpScheme)),
    ));

    let s4 = Server::new(
        "http://rpc",
        Options {
            allow_http: true,
            ..Default::default()
        },
    );
    assert!(s4.is_ok(), "http scheme with allow_http true");

    let s5 = Server::new(
        "",
        Options {
            allow_http: true,
            ..Default::default()
        },
    );
    assert!(matches!(
        s5.err(),
        Some(Error::InvalidRpc(InvalidRpcUrl::InvalidUri)),
    ));

    let s6 = Server::new(
        "http://rpc",
        Options {
            allow_http: false,
            ..Default::default()
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
    let response = json!(
    {
      "jsonrpc": "2.0",
      "id": 8675309,
      "result": {
        "status": "healthy",
        "latestLedger": 51583040,
        "oldestLedger": 51565760,
        "ledgerRetentionWindow": 17281
      }
    });
    let (s, _m) = get_mocked_server(request, response).await;
    let result = s.get_health().await.expect("Should not fail");

    let expect = GetHealthResponse {
        status: "healthy".to_string(),
        latest_ledger: 51583040,
        oldest_ledger: 51565760,
        ledger_retention_window: 17281,
    };

    assert_eq!(result, expect);
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
    assert_eq!(result, expect);
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
    let expect = GetNetworkResponse {
        friendbot_url: Some("https://friendbot-testnet.stellar.org/".into()),
        passphrase: Some("Test SDF Network ; September 2015".into()),
        protocol_version: Some(20),
    };
    assert_eq!(result, expect);
}

#[tokio::test]
async fn get_account() {
    let address = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";
    let account_id = Keypair::from_public_key(address)
        .expect("Should not fail")
        .xdr_account_id();
    let key = LedgerKey::Account(LedgerKeyAccount { account_id });
    let account_entry = "AAAAAAAAAABzdv3ojkzWHMD7KUoXhrPx0GH18vHKV0ZfqpMiEblG1gAAAFwVZH3YAAABdgAAAQgAAAAFAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAOZYQAAAAAaJsIJQ==";

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
    assert_eq!(result.sequence_number(), "1606317768968");
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
        if let Some(entries) = result.entries {
            assert_eq!(entries.len(), 1);
            let e = &entries[0];
            assert_eq!(e.last_modified_ledger_seq, Some(2));
            assert_eq!(e.to_key(), ledger_key);
            assert_eq!(e.to_data(), ledger_entry);
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
        if let Some(entries) = result.entries {
            assert_eq!(entries.len(), 1);
            let e = &entries[0];
            assert_eq!(e.last_modified_ledger_seq, Some(2));
            assert_eq!(e.to_key(), ledger_key);
            assert_eq!(e.to_data(), ledger_entry);
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
        assert!(matches!(result, Err(Error::JsonError(_))));
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

        assert_eq!(result.to_key(), ledger_key);
        assert_eq!(result.to_data(), ledger_entry);
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
            assert_eq!(r.status, TransactionStatus::Success);
            assert_eq!(r.latest_ledger, 2540076);
            assert_eq!(r.oldest_ledger, 2538637);
            assert_eq!(r.application_order, Some(1));
            let result = r.to_result().unwrap();
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
            let _envelope = r.to_envelope().expect("Should not fail");
            let (_meta, val) = r.to_result_meta().expect("Should not fail");
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
            assert_eq!(r.status, TransactionStatus::NotFound);
            assert_eq!(r.latest_ledger, 2540099);
            assert_eq!(r.oldest_ledger, 2538660);
            assert_eq!(r.application_order, None);
            assert_eq!(r.to_result(), None);
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
            assert_eq!(r.status, TransactionStatus::Failed);
            assert_eq!(r.latest_ledger, 2540124);
            assert_eq!(r.oldest_ledger, 2538685);
            assert_eq!(r.application_order, Some(2));
            let result = r.to_result().unwrap();
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
    /*
     * Events
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
            "latestLedger": 1317536,
            "latestLedgerCloseTime": "1748999063",
            "oldestLedger": 1297535,
            "oldestLedgerCloseTime": "1748898975",
            "status": "SUCCESS",
            "txHash": hash,
            "applicationOrder": 1,
            "feeBump": false,
            "envelopeXdr": "AAAAAgAAAADte5nJrehJq/pu3qlV/bASRSOiJVXdNC+gQW/nxVNWuQBY644AEETyAAAOngAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAHf65G24dyt1q+Xu3xFX5fzdHcKf3j2lXO5n11b+EnOfAAAAA1jcmVhdGVfZXNjcm93AAAAAAAABQAAAAUAAAAAAAaRmQAAABIAAAAAAAAAACcMY2GvjF3igK326WyiU8hv107p9YxvAS29gt1fml2WAAAAEgAAAAAAAAAAyewwXk7lqpxiQNYP3VlZ1EEprNK+dSBV4KQ9iluwbx8AAAASAAAAAAAAAAAY2Rm1IXXndEI0rYg2bt1/rw2mi1SYOUT2qeKPvf56cgAAABIAAAABusKzizgXRsUWKJQRrpWHAWG/yujQ6LBT/pMDljEiAegAAAAAAAAAAQAAAAAAAAACAAAABgAAAAHf65G24dyt1q+Xu3xFX5fzdHcKf3j2lXO5n11b+EnOfAAAABQAAAABAAAABw92WUOXbPOCn5SPHsgIOq8K1UypMpJe18Eh5s6eH8KeAAAAAQAAAAYAAAAB3+uRtuHcrdavl7t8RV+X83R3Cn949pVzuZ9dW/hJznwAAAAQAAAAAQAAAAIAAAAPAAAABVN0YXRlAAAAAAAABQAAAAAABpGZAAAAAQA1p5gAAEC0AAABuAAAAAAAWOsqAAAAAcVTVrkAAABAkR3EyCbHmZqEzQ1hvb1u2zY8PMqfhm7Z8zULGlpdNV0rWSbchA/NDudYEYrQKdA0qy647T+ojtdMfwLrfHELCA==",
            "resultXdr": "AAAAAABM5iEAAAAAAAAAAQAAAAAAAAAYAAAAANkPSp3CD6fXFropzD1Dse4sGrxEO/NPfv6SvhMR1kNkAAAAAA==",
            "resultMetaXdr": "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",
            "diagnosticEventsXdr": [
              "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAg3+uRtuHcrdavl7t8RV+X83R3Cn949pVzuZ9dW/hJznwAAAAPAAAADWNyZWF0ZV9lc2Nyb3cAAAAAAAAQAAAAAQAAAAUAAAAFAAAAAAAGkZkAAAASAAAAAAAAAAAnDGNhr4xd4oCt9ulsolPIb9dO6fWMbwEtvYLdX5pdlgAAABIAAAAAAAAAAMnsMF5O5aqcYkDWD91ZWdRBKazSvnUgVeCkPYpbsG8fAAAAEgAAAAAAAAAAGNkZtSF153RCNK2INm7df68NpotUmDlE9qnij73+enIAAAASAAAAAbrCs4s4F0bFFiiUEa6VhwFhv8ro0OiwU/6TA5YxIgHo",
              "AAAAAQAAAAAAAAAB3+uRtuHcrdavl7t8RV+X83R3Cn949pVzuZ9dW/hJznwAAAABAAAAAAAAAAEAAAAPAAAABGluaXQAAAAQAAAAAQAAAAUAAAAFAAAAAAAGkZkAAAASAAAAAAAAAAAnDGNhr4xd4oCt9ulsolPIb9dO6fWMbwEtvYLdX5pdlgAAABIAAAAAAAAAAMnsMF5O5aqcYkDWD91ZWdRBKazSvnUgVeCkPYpbsG8fAAAAEgAAAAAAAAAAGNkZtSF153RCNK2INm7df68NpotUmDlE9qnij73+enIAAAASAAAAAbrCs4s4F0bFFiiUEa6VhwFhv8ro0OiwU/6TA5YxIgHo",

              "AAAAAQAAAAAAAAAB3+uRtuHcrdavl7t8RV+X83R3Cn949pVzuZ9dW/hJznwAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAANY3JlYXRlX2VzY3JvdwAAAAAAAAE=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACnJlYWRfZW50cnkAAAAAAAUAAAAAAAAAAw==",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAC3dyaXRlX2VudHJ5AAAAAAUAAAAAAAAAAQ==",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEGxlZGdlcl9yZWFkX2J5dGUAAAAFAAAAAAAAQLQ=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEWxlZGdlcl93cml0ZV9ieXRlAAAAAAAABQAAAAAAAAG4",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADXJlYWRfa2V5X2J5dGUAAAAAAAAFAAAAAAAAAKg=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADndyaXRlX2tleV9ieXRlAAAAAAAFAAAAAAAAAAA=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADnJlYWRfZGF0YV9ieXRlAAAAAAAFAAAAAAAAAGg=",

              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD3dyaXRlX2RhdGFfYnl0ZQAAAAAFAAAAAAAAAbg=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADnJlYWRfY29kZV9ieXRlAAAAAAAFAAAAAAAAQEw=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD3dyaXRlX2NvZGVfYnl0ZQAAAAAFAAAAAAAAAAA=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACmVtaXRfZXZlbnQAAAAAAAUAAAAAAAAAAQ==",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD2VtaXRfZXZlbnRfYnl0ZQAAAAAFAAAAAAAAAQQ=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACGNwdV9pbnNuAAAABQAAAAAAM2BC",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACG1lbV9ieXRlAAAABQAAAAAAGwer",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEWludm9rZV90aW1lX25zZWNzAAAAAAAABQAAAAAACD99",

              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD21heF9yd19rZXlfYnl0ZQAAAAAFAAAAAAAAAFQ=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEG1heF9yd19kYXRhX2J5dGUAAAAFAAAAAAAAAbg=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEG1heF9yd19jb2RlX2J5dGUAAAAFAAAAAAAAQEw=",
              "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAE21heF9lbWl0X2V2ZW50X2J5dGUAAAAABQAAAAAAAAEE",

              "AAAAAQAAAAAAAAAB15KLcsJwPM/q9+uf9O9NUEpVqLl5/JtFDqLIQrTRzmEAAAABAAAAAAAAAAIAAAAPAAAAA2ZlZQAAAAASAAAAAAAAAADte5nJrehJq/pu3qlV/bASRSOiJVXdNC+gQW/nxVNWuQAAAAoAAAAAAAAAAAAAAAAAWOuO",
              "AAAAAQAAAAAAAAAB15KLcsJwPM/q9+uf9O9NUEpVqLl5/JtFDqLIQrTRzmEAAAABAAAAAAAAAAIAAAAPAAAAA2ZlZQAAAAASAAAAAAAAAADte5nJrehJq/pu3qlV/bASRSOiJVXdNC+gQW/nxVNWuQAAAAr/////////////////8/qT"
            ],
            "events": {
              "diagnosticEventsXdr": [
                "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAg3+uRtuHcrdavl7t8RV+X83R3Cn949pVzuZ9dW/hJznwAAAAPAAAADWNyZWF0ZV9lc2Nyb3cAAAAAAAAQAAAAAQAAAAUAAAAFAAAAAAAGkZkAAAASAAAAAAAAAAAnDGNhr4xd4oCt9ulsolPIb9dO6fWMbwEtvYLdX5pdlgAAABIAAAAAAAAAAMnsMF5O5aqcYkDWD91ZWdRBKazSvnUgVeCkPYpbsG8fAAAAEgAAAAAAAAAAGNkZtSF153RCNK2INm7df68NpotUmDlE9qnij73+enIAAAASAAAAAbrCs4s4F0bFFiiUEa6VhwFhv8ro0OiwU/6TA5YxIgHo",
                "AAAAAQAAAAAAAAAB3+uRtuHcrdavl7t8RV+X83R3Cn949pVzuZ9dW/hJznwAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAANY3JlYXRlX2VzY3JvdwAAAAAAAAE=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACnJlYWRfZW50cnkAAAAAAAUAAAAAAAAAAw==",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAC3dyaXRlX2VudHJ5AAAAAAUAAAAAAAAAAQ==",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEGxlZGdlcl9yZWFkX2J5dGUAAAAFAAAAAAAAQLQ=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEWxlZGdlcl93cml0ZV9ieXRlAAAAAAAABQAAAAAAAAG4",

                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADXJlYWRfa2V5X2J5dGUAAAAAAAAFAAAAAAAAAKg=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADndyaXRlX2tleV9ieXRlAAAAAAAFAAAAAAAAAAA=",

                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADnJlYWRfZGF0YV9ieXRlAAAAAAAFAAAAAAAAAGg=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD3dyaXRlX2RhdGFfYnl0ZQAAAAAFAAAAAAAAAbg=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADnJlYWRfY29kZV9ieXRlAAAAAAAFAAAAAAAAQEw=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD3dyaXRlX2NvZGVfYnl0ZQAAAAAFAAAAAAAAAAA=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACmVtaXRfZXZlbnQAAAAAAAUAAAAAAAAAAQ==",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD2VtaXRfZXZlbnRfYnl0ZQAAAAAFAAAAAAAAAQQ=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACGNwdV9pbnNuAAAABQAAAAAAM2BC",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACG1lbV9ieXRlAAAABQAAAAAAGwer",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEWludm9rZV90aW1lX25zZWNzAAAAAAAABQAAAAAACD99",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD21heF9yd19rZXlfYnl0ZQAAAAAFAAAAAAAAAFQ=",

                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEG1heF9yd19kYXRhX2J5dGUAAAAFAAAAAAAAAbg=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEG1heF9yd19jb2RlX2J5dGUAAAAFAAAAAAAAQEw=",
                "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAE21heF9lbWl0X2V2ZW50X2J5dGUAAAAABQAAAAAAAAEE"
              ],

              "transactionEventsXdr": [
                "AAAAAAAAAAAAAAAB15KLcsJwPM/q9+uf9O9NUEpVqLl5/JtFDqLIQrTRzmEAAAABAAAAAAAAAAIAAAAPAAAAA2ZlZQAAAAASAAAAAAAAAADte5nJrehJq/pu3qlV/bASRSOiJVXdNC+gQW/nxVNWuQAAAAoAAAAAAAAAAAAAAAAAWOuO",

                "AAAAAQAAAAAAAAAB15KLcsJwPM/q9+uf9O9NUEpVqLl5/JtFDqLIQrTRzmEAAAABAAAAAAAAAAIAAAAPAAAAA2ZlZQAAAAASAAAAAAAAAADte5nJrehJq/pu3qlV/bASRSOiJVXdNC+gQW/nxVNWuQAAAAr/////////////////8/qT"
              ],
              "contractEventsXdr": [

                [
                  "AAAAAAAAAAHf65G24dyt1q+Xu3xFX5fzdHcKf3j2lXO5n11b+EnOfAAAAAEAAAAAAAAAAQAAAA8AAAAEaW5pdAAAABAAAAABAAAABQAAAAUAAAAAAAaRmQAAABIAAAAAAAAAACcMY2GvjF3igK326WyiU8hv107p9YxvAS29gt1fml2WAAAAEgAAAAAAAAAAyewwXk7lqpxiQNYP3VlZ1EEprNK+dSBV4KQ9iluwbx8AAAASAAAAAAAAAAAY2Rm1IXXndEI0rYg2bt1/rw2mi1SYOUT2qeKPvf56cgAAABIAAAABusKzizgXRsUWKJQRrpWHAWG/yujQ6LBT/pMDljEiAeg="
                ]

              ]
            },
            "ledger": 1303424,
            "createdAt": "1748928444"

          }
        }
        );

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.get_transaction(hash).await;
        if let Ok(r) = txresult {
            if let Some((tx_events, contract_events)) = r.to_events() {
                //
                let TransactionEvent { stage, event: _ } = &tx_events[0];
                assert_eq!(stage, &TransactionEventStage::BeforeAllTxs);

                let TransactionEvent { stage, event: _ } = &tx_events[1];
                assert_eq!(stage, &TransactionEventStage::AfterTx);

                let ContractEvent {
                    ext: _,
                    contract_id,
                    type_: _,
                    body: _,
                } = &contract_events[0][0];

                assert_eq!(
                    contract_id,
                    &Some(ContractId(Hash(
                        *hex::decode(
                            "dfeb91b6e1dcadd6af97bb7c455f97f374770a7f78f69573b99f5d5bf849ce7c"
                        )
                        .unwrap()
                        .last_chunk::<32>()
                        .unwrap()
                    )))
                );
            } else {
                panic!("fail")
            }
        }
    }
}

#[tokio::test]
async fn simulate_transaction() {
    /*
     * Success (no auth)
     */
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "simulateTransaction",
          "params": {
            "transaction": tx_xdr,
            "authMode": "record"
          },
        }
        );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==",
            "minResourceFee": "90353",
            "events": [
              "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAPAAAACWluY3JlbWVudAAAAAAAABAAAAABAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAM=",
              "AAAAAQAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAAJaW5jcmVtZW50AAAAAAAAAwAAAAw="
            ],
            "results": [
              {
                "auth": [],
                "xdr": "AAAAAwAAAAw="
              }
            ],
            "cost": {
              "cpuInsns": "1635562",
              "memBytes": "1295756"
            },
            "latestLedger": 2552139
          }
        }
                );
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD",
                "10911149667123216",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();
        let time_bounds = TimeBounds {
            min_time: TimePoint(0),
            max_time: TimePoint(0),
        };

        let contract =
            Contracts::new("CDGAH7TU7UH3BXGYXRIXLJX63LYRIF6APZPIG64ZAW3NNDCPJ7AAWVTZ").unwrap();
        let op = contract.call(
            "increment",
            Some(vec![
                Address::new("GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                ScVal::U32(3),
            ]),
        );

        let mut tx_builder = TransactionBuilder::new(source_account, network, Some(time_bounds));
        tx_builder.add_operation(op);
        tx_builder.fee(100u32);

        let tx = tx_builder.build();
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s
            .simulate_transaction(
                &tx,
                Some(SimulationOptions {
                    auth_mode: Some(AuthMode::Record),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();

        if let Some((ret_val, _auth)) = txresult.to_result() {
            assert_eq!(ret_val, ScVal::U32(12));
        } else {
            panic!("Simulation failed")
        }

        if let Some(tx_data) = txresult.to_transaction_data().as_ref() {
            assert_eq!(tx_data.resource_fee, 3);
            assert_eq!(tx_data.resources.instructions, 1962674);
            assert_eq!(tx_data.resources.disk_read_bytes, 1416);
            assert_eq!(tx_data.resources.write_bytes, 136);
        } else {
            panic!("Simulation failed")
        }
    }

    /*
     * Failed transaction
     * */
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAADwAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAQAAAAMAAAADAAAAAAAAAAAAAAAA";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "simulateTransaction",
          "params": {
            "transaction": tx_xdr,
            "resourceConfig": {
              "instructionLeeway": 3000000
            },
            "authMode": "enforce"
          }
        }
                );
        let expected_err_str = "host invocation failed\n\nCaused by:\n    HostError: Error(WasmVm, InternalError)\n    \n    Event log (newest first):\n       0: [Diagnostic Event] contract:cc03fe74fd0fb0dcd8bc5175a6fedaf11417c07e5e837b9905b6d68c4f4fc00b, topics:[error, Error(WasmVm, InternalError)], data:[\"VM call failed: Func(MismatchingParameterLen)\", increment]\n       1: [Diagnostic Event] topics:[fn_call, Bytes(cc03fe74fd0fb0dcd8bc5175a6fedaf11417c07e5e837b9905b6d68c4f4fc00b), increment], data:3\n    \n    Backtrace (newest first):\n       0: soroban_env_host::vm::Vm::invoke_function_raw\n       1: soroban_env_host::host::frame::<impl soroban_env_host::host::Host>::with_frame\n       2: soroban_env_host::host::frame::<impl soroban_env_host::host::Host>::call_n_internal\n       3: soroban_env_host::host::frame::<impl soroban_env_host::host::Host>::invoke_function\n       4: preflight::preflight::preflight_invoke_hf_op\n       5: preflight::preflight_invoke_hf_op::{{closure}}\n       6: core::ops::function::FnOnce::call_once{{vtable.shim}}\n       7: preflight::catch_preflight_panic\n       8: _cgo_0b49d6ed4a0b_Cfunc_preflight_invoke_hf_op\n                 at tmp/go-build/cgo-gcc-prolog:103:11\n       9: runtime.asmcgocall\n                 at ./runtime/asm_amd64.s:848\n    \n    ";
        let response = json!(
        {
                   "jsonrpc": "2.0",
                   "id": 1,
                   "result": {
                     "error": expected_err_str,
                     "events": [
                       "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAPAAAACWluY3JlbWVudAAAAAAAAAMAAAAD",
                       "AAAAAAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAACAAAAAAAAAAIAAAAPAAAABWVycm9yAAAAAAAAAgAAAAEAAAAHAAAAEAAAAAEAAAACAAAADgAAAC1WTSBjYWxsIGZhaWxlZDogRnVuYyhNaXNtYXRjaGluZ1BhcmFtZXRlckxlbikAAAAAAAAPAAAACWluY3JlbWVudAAAAA=="
                     ],
                     "cost": {
                       "cpuInsns": "0",
                       "memBytes": "0"
                     },
                     "latestLedger": 2552013
                   }
          }
                         );
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD",
                "10911149667123214",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();
        let time_bounds = TimeBounds {
            min_time: TimePoint(0),
            max_time: TimePoint(0),
        };

        let contract =
            Contracts::new("CDGAH7TU7UH3BXGYXRIXLJX63LYRIF6APZPIG64ZAW3NNDCPJ7AAWVTZ").unwrap();
        let op = contract.call("increment", Some(vec![ScVal::U32(3)]));

        let mut tx_builder = TransactionBuilder::new(source_account, network, Some(time_bounds));
        tx_builder.add_operation(op);
        tx_builder.fee(100u32);

        let tx = tx_builder.build();
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s
            .simulate_transaction(
                &tx,
                Some(SimulationOptions {
                    cpu_instructions: 3000000,
                    auth_mode: Some(AuthMode::Enforce),
                }),
            )
            .await
            .unwrap();

        if let Some(err_str) = txresult.error.clone() {
            assert_eq!(err_str, expected_err_str);
            let diag_events = txresult.to_events().unwrap();
            assert_eq!(diag_events.len(), 2);
            let stellar_baselib::xdr::ContractEventBody::V0(ContractEventV0 { topics, data }) =
                &diag_events[0].event.body;

            assert_eq!(
                topics.to_vec()[0],
                ScVal::Symbol(ScSymbol("fn_call".try_into().unwrap()))
            );
            assert_eq!(data, &ScVal::U32(3));

            let stellar_baselib::xdr::ContractEventBody::V0(ContractEventV0 { topics, data }) =
                &diag_events[1].event.body;
            assert_eq!(
                topics.to_vec(),
                vec![
                    ScVal::Symbol(ScSymbol("error".try_into().unwrap())),
                    ScVal::Error(stellar_baselib::xdr::ScError::WasmVm(
                        stellar_baselib::xdr::ScErrorCode::InternalError
                    ))
                ]
            );
            if let ScVal::Vec(Some(data_v)) = data {
                assert_eq!(
                    data_v.to_vec(),
                    vec![
                        ScVal::String(ScString(
                            "VM call failed: Func(MismatchingParameterLen)"
                                .try_into()
                                .unwrap()
                        )),
                        ScVal::Symbol(ScSymbol("increment".try_into().unwrap()))
                    ]
                )
            } else {
                panic!("Missing diag event")
            }
        } else {
            panic!("Simulation failed")
        }
    }
    /*
     * State changes
     */
    {
        let tx_xdr = "AAAAAgAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwX14QAAD1DYAAAAAgAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAAAhpbmNfYXV0aAAAAAEAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAAAAAAAAA==";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "simulateTransaction",
          "params": {
            "transaction": tx_xdr,
            "authMode": "record_allow_nonroot"
          }
        });
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 8675309,
          "result": {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAFAAAAAEAAAAHcOiuro2Kjk7NwMT6FDrXvb/h7SFI2ZYIxVt7UQy0M6EAAAABAAAABgAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAABAAAAABAAAAAgAAAA8AAAALQ291bnRlckF1dGgAAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAAAA0kKAAABrAAAACMAAAAAAABxsc=",
            "minResourceFee": "116423",
            "events": [
                    "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgJCCueCLXNyGRkbQYp3du07sHifpJQ4j8iuTV7uuwmmgAAAAPAAAACGluY19hdXRoAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XM=",
                    "AAAAAQAAAAAAAAABJCCueCLXNyGRkbQYp3du07sHifpJQ4j8iuTV7uuwmmgAAAABAAAAAAAAAAEAAAAPAAAAB2NvdW50ZXIAAAAAEAAAAAEAAAACAAAAEAAAAAEAAAACAAAADwAAAAtDb3VudGVyQXV0aAAAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAMAAAAB",
                    "AAAAAQAAAAAAAAABJCCueCLXNyGRkbQYp3du07sHifpJQ4j8iuTV7uuwmmgAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAAIaW5jX2F1dGgAAAADAAAAAQ==",
            ],
            "results": [
              {
                "auth": [
                            "AAAAAAAAAAAAAAABJCCueCLXNyGRkbQYp3du07sHifpJQ4j8iuTV7uuwmmgAAAAIaW5jX2F1dGgAAAABAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAA",
                        ],
                "xdr": "AAAAAwAAAAE=",
              }
            ],
            "stateChanges":
                [
                    {
                        "type": "created",
                        "key": "AAAABgAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAABAAAAABAAAAAgAAAA8AAAALQ291bnRlckF1dGgAAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAA",
                        "before": null,
                        "after":
                            "AAAAAAAAAAYAAAAAAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAEAAAAAEAAAACAAAADwAAAAtDb3VudGVyQXV0aAAAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAADAAAAAQAAAAA=",
                    },
                ],
            "latestLedger": 2552139
          }
        });

        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GD7UIMVKR6RJ3HNJE2PFNHH2EWAUJYZDPBHUL74W2CM7J6A3YDSXGPJN",
                "4311013293817857",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();

        let contract =
            Contracts::new("CASCBLTYELLTOIMRSG2BRJ3XN3J3WB4J7JEUHCH4RLSNL3XLWCNGRTCR").unwrap();
        let op = contract.call(
            "inc_auth",
            Some(vec![Address::new(
                "GD7UIMVKR6RJ3HNJE2PFNHH2EWAUJYZDPBHUL74W2CM7J6A3YDSXGPJN",
            )
            .unwrap()
            .to_sc_val()
            .unwrap()]),
        );

        let mut tx_builder = TransactionBuilder::new(source_account, network, None);
        tx_builder.add_operation(op);
        tx_builder.fee(100000000u32);

        let tx = tx_builder.build();
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s
            .simulate_transaction(
                &tx,
                Some(SimulationOptions {
                    auth_mode: Some(AuthMode::RecordAllowNonRoot),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        let state_changes = txresult.to_state_changes();
        assert_eq!(state_changes.len(), 1);
        assert!(matches!(&state_changes[0].kind, StateChangeKind::Created));
    }
    /*
     * Updated
     */
    {
        let tx_xdr = "AAAAAgAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwX14QAAD1DYAAAAAwAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAAAhpbmNfYXV0aAAAAAEAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAAAAAAAAA==";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "simulateTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        });
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 8675309,
          "result": {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAFAAAAAEAAAAHcOiuro2Kjk7NwMT6FDrXvb/h7SFI2ZYIxVt7UQy0M6EAAAABAAAABgAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAABAAAAABAAAAAgAAAA8AAAALQ291bnRlckF1dGgAAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAAAA0kKAAABrAAAACMAAAAAAABxsc=",
            "minResourceFee": "116423",
            "results": [
              {
                "auth": [
                           "AAAAAAAAAAAAAAABJCCueCLXNyGRkbQYp3du07sHifpJQ4j8iuTV7uuwmmgAAAAIaW5jX2F1dGgAAAABAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAA",
                        ],
                "xdr": "AAAAAwAAAAI=",
              }
            ],
            "stateChanges":
                [
                    {
                        "type": "updated",
                        "key": "AAAABgAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAABAAAAABAAAAAgAAAA8AAAALQ291bnRlckF1dGgAAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAA",
                        "before":
                            "AA9Q2wAAAAYAAAAAAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAEAAAAAEAAAACAAAADwAAAAtDb3VudGVyQXV0aAAAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAADAAAAAQAAAAA=",
                        "after":
                            "AA9Q2wAAAAYAAAAAAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAEAAAAAEAAAACAAAADwAAAAtDb3VudGVyQXV0aAAAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAADAAAAAgAAAAA=",
                    },
                ],
            "latestLedger": 2552139
          }
        });

        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GD7UIMVKR6RJ3HNJE2PFNHH2EWAUJYZDPBHUL74W2CM7J6A3YDSXGPJN",
                "4311013293817858",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();

        let contract =
            Contracts::new("CASCBLTYELLTOIMRSG2BRJ3XN3J3WB4J7JEUHCH4RLSNL3XLWCNGRTCR").unwrap();
        let op = contract.call(
            "inc_auth",
            Some(vec![Address::new(
                "GD7UIMVKR6RJ3HNJE2PFNHH2EWAUJYZDPBHUL74W2CM7J6A3YDSXGPJN",
            )
            .unwrap()
            .to_sc_val()
            .unwrap()]),
        );

        let mut tx_builder = TransactionBuilder::new(source_account, network, None);
        tx_builder.add_operation(op);
        tx_builder.fee(100000000u32);

        let tx = tx_builder.build();
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s
            .simulate_transaction(
                &tx,
                Some(SimulationOptions {
                    auth_mode: None,
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        let state_changes = txresult.to_state_changes();
        assert_eq!(state_changes.len(), 1);
        assert!(matches!(&state_changes[0].kind, StateChangeKind::Updated));
    }
    /*
     * Deleted
     */
    {
        let tx_xdr = "AAAAAgAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwX14QAAD1DYAAAABAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAAAhpbmNfYXV0aAAAAAEAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAAAAAAAAA==";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "simulateTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        });
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAFAAAAAEAAAAHcOiuro2Kjk7NwMT6FDrXvb/h7SFI2ZYIxVt7UQy0M6EAAAABAAAABgAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAABAAAAABAAAAAgAAAA8AAAALQ291bnRlckF1dGgAAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAAAA0H8gAABzwAAAAAAAAAAAABdsc=",
            "minResourceFee": "116423",
            "results": [
              {
                "auth": [
                            "AAAAAAAAAAAAAAABJCCueCLXNyGRkbQYp3du07sHifpJQ4j8iuTV7uuwmmgAAAAIaW5jX2F1dGgAAAABAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAA",
                        ],
                "xdr": "AAAAAwAAAAM=",
              }
            ],
            "stateChanges":
                [
                    {
                        "type": "deleted",
                        "key": "AAAABgAAAAEkIK54Itc3IZGRtBind27TuweJ+klDiPyK5NXu67CaaAAAABAAAAABAAAAAgAAAA8AAAALQ291bnRlckF1dGgAAAAAEgAAAAAAAAAA/0Qyqo+inZ2pJp5WnPolgUTjI3hPRf+W0Jn0+BvA5XMAAAAA",
                        "before":
                            "AA9Q3AAAAAYAAAAAAAAAASQgrngi1zchkZG0GKd3btO7B4n6SUOI/Irk1e7rsJpoAAAAEAAAAAEAAAACAAAADwAAAAtDb3VudGVyQXV0aAAAAAASAAAAAAAAAAD/RDKqj6Kdnakmnlac+iWBROMjeE9F/5bQmfT4G8DlcwAAAAAAAAADAAAAAgAAAAA=",
                        "after": null,
                    },
                ],
            "latestLedger": 2552139
          }
        });

        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GD7UIMVKR6RJ3HNJE2PFNHH2EWAUJYZDPBHUL74W2CM7J6A3YDSXGPJN",
                "4311013293817859",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();

        let contract =
            Contracts::new("CASCBLTYELLTOIMRSG2BRJ3XN3J3WB4J7JEUHCH4RLSNL3XLWCNGRTCR").unwrap();
        let op = contract.call(
            "inc_auth",
            Some(vec![Address::new(
                "GD7UIMVKR6RJ3HNJE2PFNHH2EWAUJYZDPBHUL74W2CM7J6A3YDSXGPJN",
            )
            .unwrap()
            .to_sc_val()
            .unwrap()]),
        );

        let mut tx_builder = TransactionBuilder::new(source_account, network, None);
        tx_builder.add_operation(op);
        tx_builder.fee(100000000u32);

        let tx = tx_builder.build();
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.simulate_transaction(&tx, None).await.unwrap();
        let state_changes = txresult.to_state_changes();
        assert_eq!(state_changes.len(), 1);
        assert!(matches!(&state_changes[0].kind, StateChangeKind::Deleted));
        // TODO more test
    }
}

#[tokio::test]
async fn send_transaction() {
    /*
     * Pending transaction
     */
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAADQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAACgAAAAVIZWxsbwAAAAAAAAEAAAAMU29yb2JhbiBEb2NzAAAAAAAAAAELm/oYAAAAQATr6Ghp/DNO7S6JjEFwcJ9a+dvI6NJr7I/2eQttvoovjQ8te4zKKaapC3mbmx6ld6YKL5T81mxs45TjzdG5zw0=";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "sendTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        }
            );
        let hash = "d8ec9b68780314ffdfdfc2194b1b35dd27d7303c3bceaef6447e31631a1419dc";
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "status": "PENDING",
            "hash": hash,
            "latestLedger": 2553978,
            "latestLedgerCloseTime": "1700159337"
          }
        }
            );

        let network = Networks::testnet();
        let tx = Transaction::from_xdr_envelope(tx_xdr, network);
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();

        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.send_transaction(tx).await.unwrap();
        assert_eq!(txresult.status, SendTransactionStatus::Pending);
        assert_eq!(txresult.hash, hash);
        assert_eq!(txresult.latest_ledger, 2553978);
        assert_eq!(txresult.latest_ledger_close_time, "1700159337");
        assert_eq!(txresult.to_error_result(), None);
        assert_eq!(txresult.to_diagnostic_events(), None);
    }

    /*
     * Duplicate transaction
     */
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAADQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAACgAAAAVIZWxsbwAAAAAAAAEAAAAMU29yb2JhbiBEb2NzAAAAAAAAAAELm/oYAAAAQATr6Ghp/DNO7S6JjEFwcJ9a+dvI6NJr7I/2eQttvoovjQ8te4zKKaapC3mbmx6ld6YKL5T81mxs45TjzdG5zw0=";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "sendTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        }
                    );
        let hash = "d8ec9b68780314ffdfdfc2194b1b35dd27d7303c3bceaef6447e31631a1419dc";
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "status": "DUPLICATE",
            "hash": hash,
            "latestLedger": 2553978,
            "latestLedgerCloseTime": "1700159337"
          }
        }
            );

        let network = Networks::testnet();
        let tx = Transaction::from_xdr_envelope(tx_xdr, network);
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();

        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.send_transaction(tx).await.unwrap();
        assert_eq!(txresult.status, SendTransactionStatus::Duplicate);
        assert_eq!(txresult.hash, hash);
        assert_eq!(txresult.latest_ledger, 2553978);
        assert_eq!(txresult.latest_ledger_close_time, "1700159337");
        assert_eq!(txresult.to_error_result(), None);
        assert_eq!(txresult.to_diagnostic_events(), None);
    }
    /*
     * Try again transaction
     */
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAADQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAACgAAAAVIZWxsbwAAAAAAAAEAAAAMU29yb2JhbiBEb2NzAAAAAAAAAAELm/oYAAAAQATr6Ghp/DNO7S6JjEFwcJ9a+dvI6NJr7I/2eQttvoovjQ8te4zKKaapC3mbmx6ld6YKL5T81mxs45TjzdG5zw0=";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "sendTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        }
                    );
        let hash = "d8ec9b68780314ffdfdfc2194b1b35dd27d7303c3bceaef6447e31631a1419dc";
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "status": "TRY_AGAIN_LATER",
            "hash": hash,
            "latestLedger": 2553978,
            "latestLedgerCloseTime": "1700159337"
          }
        }
            );

        let network = Networks::testnet();
        let tx = Transaction::from_xdr_envelope(tx_xdr, network);
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();

        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.send_transaction(tx).await.unwrap();
        assert_eq!(txresult.status, SendTransactionStatus::TryAgainLater);
        assert_eq!(txresult.hash, hash);
        assert_eq!(txresult.latest_ledger, 2553978);
        assert_eq!(txresult.latest_ledger_close_time, "1700159337");
        assert_eq!(txresult.to_error_result(), None);
        assert_eq!(txresult.to_diagnostic_events(), None);
    }

    /*
     * Error transaction
     */
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAAAAAAAAACgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAACgAAAAVIZWxsbwAAAAAAAAEAAAAMU29yb2JhbiBEb2NzAAAAAAAAAAELm/oYAAAAQMQkfl8sdCYQIOdJB0TyazJ126y2TFRjL8yNHSb4TTsH5Ym6qM6gkTx1ENRZ0PFprVGusMTHISzdPHYJ4njBZAQ=";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "sendTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        }
                            );
        let hash = "84a5f62bff422581dda019811daed0868a3db41833ad6e90a12f0d7db1be8167";
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "errorResultXdr": "AAAAAAAAAGT////7AAAAAA==",
            "status": "ERROR",
            "hash": hash,
            "latestLedger": 2553978,
            "latestLedgerCloseTime": "1700159337"
          }
        }
                    );

        let network = Networks::testnet();
        let tx = Transaction::from_xdr_envelope(tx_xdr, network);
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();

        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.send_transaction(tx).await.unwrap();
        assert_eq!(txresult.status, SendTransactionStatus::Error);
        assert_eq!(txresult.hash, hash);
        assert_eq!(txresult.latest_ledger, 2553978);
        assert_eq!(txresult.latest_ledger_close_time, "1700159337");
        assert_eq!(txresult.to_diagnostic_events(), None);

        let tx_error = TransactionResult {
            fee_charged: 100,
            result: TransactionResultResult::TxBadSeq,
            ext: stellar_baselib::xdr::TransactionResultExt::V0,
        };
        assert_eq!(txresult.to_error_result(), Some(tx_error));
    }
}

#[tokio::test]
async fn prepare_transaction() {
    {
        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=";
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "simulateTransaction",
          "params": {
            "transaction": tx_xdr,
          }
        }
        );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==",
            "minResourceFee": "90353",
            "events": [
              "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAPAAAACWluY3JlbWVudAAAAAAAABAAAAABAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAM=",
              "AAAAAQAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAAJaW5jcmVtZW50AAAAAAAAAwAAAAw="
            ],
            "results": [
              {
                "auth": [],
                "xdr": "AAAAAwAAAAw="
              }
            ],
            "cost": {
              "cpuInsns": "1635562",
              "memBytes": "1295756"
            },
            "latestLedger": 2552139
          }
        }
        );
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD",
                "10911149667123217",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();
        let time_bounds = TimeBounds {
            min_time: TimePoint(0),
            max_time: TimePoint(0),
        };

        let contract =
            Contracts::new("CDGAH7TU7UH3BXGYXRIXLJX63LYRIF6APZPIG64ZAW3NNDCPJ7AAWVTZ").unwrap();
        let op = contract.call(
            "increment",
            Some(vec![
                Address::new("GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                ScVal::U32(3),
            ]),
        );

        let mut tx_builder = TransactionBuilder::new(source_account, network, Some(time_bounds));
        tx_builder.add_operation(op);
        tx_builder.fee(100u32);

        let tx = tx_builder.build();
        dbg!(&tx);
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;

        let simulation = s.simulate_transaction(&tx, None).await.unwrap();
        let txresult = s.prepare_transaction(&tx).await.unwrap();

        assert_eq!(txresult.fee, tx.fee + 90353);
        assert_eq!(txresult.soroban_data, simulation.to_transaction_data());
    }
    {
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD",
                "10911149667123217",
            )
            .unwrap(),
        ));
        let network = Networks::testnet();
        let time_bounds = TimeBounds {
            min_time: TimePoint(0),
            max_time: TimePoint(0),
        };

        let contract =
            Contracts::new("CDGAH7TU7UH3BXGYXRIXLJX63LYRIF6APZPIG64ZAW3NNDCPJ7AAWVTZ").unwrap();
        let op = contract.call(
            "increment",
            Some(vec![
                Address::new("GAQODVWAY3AYAGEAT4CG3YSPM4FBTBB2QSXCYJLM3HVIV5ILTP5BRXCD")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                ScVal::U32(3),
            ]),
        );

        /*
         * This is some fake data
         */
        let key_to_restore = ScVal::Vec(Some(ScVec(
            [ScVal::Symbol("to_restore".try_into().unwrap())]
                .try_into()
                .unwrap(),
        )));
        let contract_data = LedgerKey::ContractData(LedgerKeyContractData {
            contract: contract.address().to_sc_address().unwrap(),
            key: key_to_restore,
            durability: stellar_baselib::xdr::ContractDataDurability::Persistent,
        });
        let soroban_data = SorobanTransactionData {
            ext: stellar_baselib::xdr::SorobanTransactionDataExt::V0,
            resources: SorobanResources {
                footprint: stellar_baselib::xdr::LedgerFootprint {
                    read_only: Vec::new().try_into().unwrap(),
                    read_write: vec![contract_data].try_into().unwrap(),
                },
                instructions: 0,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 0,
        };
        let restore_data_xdr = soroban_data.to_xdr_base64(Limits::none()).unwrap();

        let tx_xdr = "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=";
        let request = json!(
                {
                  "jsonrpc": "2.0",
                  "id": 1,
                  "method": "simulateTransaction",
                  "params": {
                    "transaction": tx_xdr,
                  }
                }
                /*
                    "resourceConfig": {
                      "instructionLeeway": 3000000
                    }
        */
                                );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==",
            "minResourceFee": "90353",
            "events": [
              "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAPAAAACWluY3JlbWVudAAAAAAAABAAAAABAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAM=",
              "AAAAAQAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAAJaW5jcmVtZW50AAAAAAAAAwAAAAw="
            ],
            "results": [
              {
                "auth": [],
                "xdr": "AAAAAwAAAAw="
              }
            ],
            "restorePreamble": {
                "minResourceFee" : "12345",
                "transactionData": restore_data_xdr,
            },
            "cost": {
              "cpuInsns": "1635562",
              "memBytes": "1295756"
            },
            "latestLedger": 2552139
          }
        }
                );

        let mut tx_builder = TransactionBuilder::new(source_account, network, Some(time_bounds));
        tx_builder.add_operation(op);
        tx_builder.fee(100u32);

        let tx = tx_builder.build();
        let xdr = tx
            .to_envelope()
            .unwrap()
            .to_xdr_base64(Limits::none())
            .unwrap();
        assert_eq!(xdr, tx_xdr);

        let (s, _m) = get_mocked_server(request, response).await;
        let txresult = s.prepare_transaction(&tx).await;

        if let Err(Error::RestorationRequired(min_fee, transaction_data)) = txresult {
            assert_eq!(min_fee, 12345);
            assert_eq!(transaction_data, soroban_data);
        } else {
            panic!("Expecting a restore preamble")
        }
    }
}

#[tokio::test]
async fn get_events() {
    /*
     *
     */
    {
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "getEvents",
          "params": {
            "startLedger": 1190000,
            "filters": [
              {
                "type": "contract",
                "contractIds": [
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
                ],
                "topics": [
                  [
                    "AAAADwAAAAh0cmFuc2Zlcg==",
                    "*",
                    "*",
                    "*"
                  ]
                ]
              }
            ],
            "pagination": {
              "limit": 2
            }
          }
        }
                );

        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "events": [
              {
                "type": "contract",
                "ledger": 1192402,
                "ledgerClosedAt": "2024-04-19T06:19:00Z",
                "contractId": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                "id": "0005121327593697280-0000000002",
                "pagingToken": "0005121327593697280-0000000002",
                "topic": [
                  "AAAADwAAAAh0cmFuc2Zlcg==",
                  "AAAAEgAAAAAAAAAA/ItF3cwsGhkee6lL5X6kicifdluxtsuDq9J/F0zou6Q=",
                  "AAAAEgAAAAAAAAAA2uSCoRk2StNEYAmWOhGXKQg0SLV6avaiN7TdV/uMozQ=",
                  "AAAADgAAAAZuYXRpdmUAAA=="
                ],
                "value": "AAAACgAAAAAAAAAAAAAAAAX14QA=",
                "inSuccessfulContractCall": true,
                "txHash": "bd8a6611a3b44ff7dce76a27fb32fa69c9f6eff3435ca5296f6035d5105617ed"
              },
              {
                "type": "contract",
                "ledger": 1192438,
                "ledgerClosedAt": "2024-04-19T06:22:14Z",
                "contractId": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                "id": "0005121482212511744-0000000002",
                "pagingToken": "0005121482212511744-0000000002",
                "topic": [
                  "AAAADwAAAAh0cmFuc2Zlcg==",
                  "AAAAEgAAAAAAAAAA/ItF3cwsGhkee6lL5X6kicifdluxtsuDq9J/F0zou6Q=",
                  "AAAAEgAAAAAAAAAA2uSCoRk2StNEYAmWOhGXKQg0SLV6avaiN7TdV/uMozQ=",
                  "AAAADgAAAAZuYXRpdmUAAA=="
                ],
                "value": "AAAACgAAAAAAAAAAAAAAAACYloA=",
                "inSuccessfulContractCall": true,
                "txHash": "3fa1787085a4aa585507751394d70494a02e71d347446095a763ecbfc647937c"
              }
            ],
            "latestLedger": 1197926
          }
        }
                );

        let (s, _m) = get_mocked_server(request, response).await;

        let ledger = crate::Pagination::From(1190000);
        let contract_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
        let filters = vec![EventFilter::new(crate::soroban_rpc::EventType::Contract)
            .contract(contract_id)
            .topic(vec![
                Topic::Val(ScVal::Symbol(ScSymbol("transfer".try_into().unwrap()))),
                Topic::Any,
                Topic::Any,
                Topic::Any,
            ])];
        let result = s.get_events(ledger, filters, 2).await.unwrap();

        let events = result.events;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].contract_id, contract_id);
        assert_eq!(events[0].ledger, 1192402);
        assert_eq!(
            events[0].topic(),
            vec![
                ScVal::Symbol(ScSymbol("transfer".try_into().unwrap())),
                Address::new("GD6IWRO5ZQWBUGI6POUUXZL6USE4RH3WLOY3NS4DVPJH6F2M5C52IYAI")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                Address::new("GDNOJAVBDE3EVU2EMAEZMOQRS4UQQNCIWV5GV5VCG62N2V73RSRTJMP6")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                ScVal::String(ScString("native".try_into().unwrap()))
            ]
        );
        assert_eq!(
            events[0].value(),
            ScVal::I128(Int128Parts {
                hi: 0,
                lo: 100000000
            })
        );

        assert_eq!(
            events[1].topic(),
            vec![
                ScVal::Symbol(ScSymbol("transfer".try_into().unwrap())),
                Address::new("GD6IWRO5ZQWBUGI6POUUXZL6USE4RH3WLOY3NS4DVPJH6F2M5C52IYAI")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                Address::new("GDNOJAVBDE3EVU2EMAEZMOQRS4UQQNCIWV5GV5VCG62N2V73RSRTJMP6")
                    .unwrap()
                    .to_sc_val()
                    .unwrap(),
                ScVal::String(ScString("native".try_into().unwrap()))
            ]
        );
        assert_eq!(events[1].contract_id, contract_id);
        assert_eq!(events[1].ledger, 1192438);
        assert_eq!(
            events[1].value(),
            ScVal::I128(Int128Parts {
                hi: 0,
                lo: 10000000
            })
        );
    }
    /*
     * Too much contracts
     */
    {
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "method": "getEvents",
          "params": {
            "startLedger": 1190000,
            "filters": [
              {
                "type": "contract",
                "contractIds": [
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                ],
              }
            ],
            "pagination": {
              "limit": 2
            }
          }
        }
                );
        let response = json!(
            {
              "jsonrpc": "2.0",
              "id": 1,
        "error":      {
            "code": -32602,
            "message": "filter 1 invalid: maximum 5 contract IDs per filter",
        },
        });

        let (s, _m) = get_mocked_server(request, response).await;

        let ledger = crate::Pagination::From(1190000);
        let contract_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
        let filters = vec![EventFilter::new(crate::soroban_rpc::EventType::Contract)
            .contract(contract_id)
            .contract(contract_id)
            .contract(contract_id)
            .contract(contract_id)
            .contract(contract_id)
            .contract(contract_id)];
        let result = s.get_events(ledger, filters, 2).await;

        if let Err(Error::RPCError { code, message }) = result {
            assert_eq!(code, -32602);
            assert_eq!(
                message,
                "filter 1 invalid: maximum 5 contract IDs per filter"
            );
        } else {
            panic!("Expecting an RPCError")
        }
    }
    // TODO more tests
}

#[tokio::test]
async fn get_fee_stats() {
    let request = json!({
      "jsonrpc": "2.0",
      "id": 1,
      "method": "getFeeStats"
    });
    let response = json!({
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "sorobanInclusionFee": {
          "max": "210",
          "min": "100",
          "mode": "100",
          "p10": "100",
          "p20": "100",
          "p30": "100",
          "p40": "100",
          "p50": "100",
          "p60": "100",
          "p70": "100",
          "p80": "100",
          "p90": "120",
          "p95": "190",
          "p99": "200",
          "transactionCount": "10",
          "ledgerCount": 50
        },
        "inclusionFee": {
          "max": "100",
          "min": "100",
          "mode": "100",
          "p10": "100",
          "p20": "100",
          "p30": "100",
          "p40": "100",
          "p50": "100",
          "p60": "100",
          "p70": "100",
          "p80": "100",
          "p90": "100",
          "p95": "100",
          "p99": "100",
          "transactionCount": "7",
          "ledgerCount": 10
        },
        "latestLedger": 4519945
      }
    });

    let (s, _m) = get_mocked_server(request, response).await;
    let response = s.get_fee_stats().await.unwrap();

    assert_eq!(response.inclusion_fee.ledger_count, 10);
    assert_eq!(response.inclusion_fee.transaction_count, "7");
    assert_eq!(response.inclusion_fee.max, "100");
    assert_eq!(response.inclusion_fee.min, "100");
    assert_eq!(response.inclusion_fee.mode, "100");
    assert_eq!(response.inclusion_fee.p10, "100");
    assert_eq!(response.inclusion_fee.p20, "100");
    assert_eq!(response.inclusion_fee.p30, "100");
    assert_eq!(response.inclusion_fee.p40, "100");
    assert_eq!(response.inclusion_fee.p50, "100");
    assert_eq!(response.inclusion_fee.p60, "100");
    assert_eq!(response.inclusion_fee.p70, "100");
    assert_eq!(response.inclusion_fee.p80, "100");
    assert_eq!(response.inclusion_fee.p90, "100");
    assert_eq!(response.inclusion_fee.p95, "100");
    assert_eq!(response.inclusion_fee.p99, "100");

    assert_eq!(response.soroban_inclusion_fee.ledger_count, 50);
    assert_eq!(response.soroban_inclusion_fee.transaction_count, "10");
    assert_eq!(response.soroban_inclusion_fee.max, "210");
    assert_eq!(response.soroban_inclusion_fee.min, "100");
    assert_eq!(response.soroban_inclusion_fee.mode, "100");
    assert_eq!(response.soroban_inclusion_fee.p10, "100");
    assert_eq!(response.soroban_inclusion_fee.p20, "100");
    assert_eq!(response.soroban_inclusion_fee.p30, "100");
    assert_eq!(response.soroban_inclusion_fee.p40, "100");
    assert_eq!(response.soroban_inclusion_fee.p50, "100");
    assert_eq!(response.soroban_inclusion_fee.p60, "100");
    assert_eq!(response.soroban_inclusion_fee.p70, "100");
    assert_eq!(response.soroban_inclusion_fee.p80, "100");
    assert_eq!(response.soroban_inclusion_fee.p90, "120");
    assert_eq!(response.soroban_inclusion_fee.p95, "190");
    assert_eq!(response.soroban_inclusion_fee.p99, "200");
}

#[tokio::test]
async fn get_version_info() {
    let request = json!({
      "jsonrpc": "2.0",
      "id": 1,
      "method": "getVersionInfo"
    });
    let response = json!({
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "version": "21.1.0",
        "commit_hash": "fcd2f0523f04279bae4502f3e3fa00ca627e6f6a",
        "build_time_stamp": "2024-05-10T11:18:38",
        "captive_core_version": "stellar-core 21.0.0.rc2 (c6f474133738ae5f6d11b07963ca841909210273)",
        "protocol_version": 21
      }
    });

    let (s, _m) = get_mocked_server(request, response).await;
    let response = s.get_version_info().await.unwrap();

    assert_eq!(response.version, "21.1.0");
    assert_eq!(
        response.commit_hash,
        "fcd2f0523f04279bae4502f3e3fa00ca627e6f6a"
    );
    assert_eq!(response.protocol_version, 21);
}

#[tokio::test]
async fn request_airdrop() {
    /*
     * No friendbot on mainnet
     */
    {
        let request = json!({"method": "getNetwork"});
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "passphrase": "Public Global Stellar Network ; September 2015",
                    "protocolVersion": 22,
            }
        }
        );
        let (s, _m) = get_mocked_server(request, response).await;
        let kp = Keypair::random().unwrap();
        let account_id = kp.public_key();
        let result = s.request_airdrop(&account_id).await;
        assert!(matches!(result.err(), Some(Error::NoFriendbot)));
    }
    /*
     * Failed tx
     */
    {
        let account_id = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";
        let friendly_server = get_friendbot_server(account_id, json!({"successful": false})).await;
        let uri = friendly_server.uri();
        let request = json!({"method": "getNetwork"});
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "friendbotUrl": uri,
                    "passphrase": "Test SDF Network ; September 2015",
                    "protocolVersion": 20
                }
            }
        );
        let (s, _m) = get_mocked_server(request, response).await;
        let result = s.request_airdrop(account_id).await;
        assert!(matches!(result.err(), Some(Error::AccountNotFound)));
    }
    /*
     * Found url from testnet
     */
    {
        let account_id = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";

        let friendly_server = get_friendbot_server(account_id, json!({"successful": true})).await;
        let uri = friendly_server.uri();
        let request = json!({"method": "getNetwork"});
        let response = json!(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "friendbotUrl": uri,
                    "passphrase": "Test SDF Network ; September 2015",
                    "protocolVersion": 20
                }
            }
        );
        let (s, mock_server) = get_mocked_server(request, response).await;

        let account_id_xdr = Keypair::from_public_key(account_id)
            .expect("Should not fail")
            .xdr_account_id();
        let key = LedgerKey::Account(LedgerKeyAccount {
            account_id: account_id_xdr,
        });
        let account_entry = "AAAAAAAAAABzdv3ojkzWHMD7KUoXhrPx0GH18vHKV0ZfqpMiEblG1gAAAFwVZH3YAAABdgAAAQgAAAAFAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAOZYQAAAAAaJsIJQ==";

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
        let response = ResponseTemplate::new(200).set_body_json(response);
        Mock::given(method("POST"))
            .and(path("/"))
            .and(matchers::body_partial_json(request))
            .respond_with(response)
            .expect(1..)
            .mount(&mock_server)
            .await;

        let result = s.request_airdrop(account_id).await.unwrap();
        assert_eq!(result.sequence_number(), "1606317768968");
        assert_eq!(result.account_id(), account_id);
    }
}
#[tokio::test]
async fn get_transactions() {
    {
        //
        let request = json!(
        {
          "jsonrpc": "2.0",
          "method": "getTransactions",
          "params": {
            "startLedger": 1888539,
            "pagination": {
              "limit": 5
            }
          }
        }
            );
        let response = json!(
        {
          "jsonrpc": "2.0",
          "id": 1,
          "result": {
            "transactions": [
              {
                "status": "FAILED",
                "applicationOrder": 1,
                "feeBump": false,
                "envelopeXdr": "AAAAAgAAAACDz21Q3CTITlGqRus3/96/05EDivbtfJncNQKt64BTbAAAASwAAKkyAAXlMwAAAAEAAAAAAAAAAAAAAABmWeASAAAAAQAAABR3YWxsZXQ6MTcxMjkwNjMzNjUxMAAAAAEAAAABAAAAAIPPbVDcJMhOUapG6zf/3r/TkQOK9u18mdw1Aq3rgFNsAAAAAQAAAABwOSvou8mtwTtCkysVioO35TSgyRir2+WGqO8FShG/GAAAAAFVQUgAAAAAAO371tlrHUfK+AvmQvHje1jSUrvJb3y3wrJ7EplQeqTkAAAAAAX14QAAAAAAAAAAAeuAU2wAAABAn+6A+xXvMasptAm9BEJwf5Y9CLLQtV44TsNqS8ocPmn4n8Rtyb09SBiFoMv8isYgeQU5nAHsIwBNbEKCerusAQ==",
                "resultXdr": "AAAAAAAAAGT/////AAAAAQAAAAAAAAAB////+gAAAAA=",
                "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAc0RsAAAAAAAAAAIPPbVDcJMhOUapG6zf/3r/TkQOK9u18mdw1Aq3rgFNsAAAAF0YpYBQAAKkyAAXlMgAAAAsAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAABzRGgAAAABmWd/VAAAAAAAAAAEAHNEbAAAAAAAAAACDz21Q3CTITlGqRus3/96/05EDivbtfJncNQKt64BTbAAAABdGKWAUAACpMgAF5TMAAAALAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAc0RsAAAAAZlnf2gAAAAAAAAAAAAAAAAAAAAA=",
                "ledger": 1888539,
                "createdAt": 1717166042
              },
              {
                "status": "SUCCESS",
                "applicationOrder": 2,
                "feeBump": false,
                "envelopeXdr": "AAAAAgAAAAC4EZup+ewCs/doS3hKbeAa4EviBHqAFYM09oHuLtqrGAAPQkAAGgQZAAAANgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAABAAAAABB90WssODNIgi6BHveqzxTRmIpvAFRyVNM+Hm2GVuCcAAAAAAAAAAAq6aHAHZ2sd9aPbRsskrlXMLWIwqs4Sv2Bk+VwuIR+9wAAABdIdugAAAAAAAAAAAIu2qsYAAAAQERzKOqYYiPXNwsiL8ADAG/f45RBssmf3umGzw4qKkLGlObuPdX0buWmTGrhI13SG38F2V8Mp9DI+eDkcCjMSAOGVuCcAAAAQHnm0o/r+Gsl+6oqBgSbqoSY37gflvQB3zZRghuir0N75UVerd0Q50yG5Zfu08i2crhx6uk+5HYTl8/Sa7uZ+Qc=",
                "resultXdr": "AAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAA=",
                "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAc0RsAAAAAAAAAALgRm6n57AKz92hLeEpt4BrgS+IEeoAVgzT2ge4u2qsYAAAAADwzS2gAGgQZAAAANQAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAABzPVAAAAABmWdZ2AAAAAAAAAAEAHNEbAAAAAAAAAAC4EZup+ewCs/doS3hKbeAa4EviBHqAFYM09oHuLtqrGAAAAAA8M0toABoEGQAAADYAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAc0RsAAAAAZlnf2gAAAAAAAAABAAAAAwAAAAMAHNEaAAAAAAAAAAAQfdFrLDgzSIIugR73qs8U0ZiKbwBUclTTPh5thlbgnABZJUSd0V2hAAAAawAAAlEAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAaBGEAAAAAZkspCwAAAAAAAAABABzRGwAAAAAAAAAAEH3Rayw4M0iCLoEe96rPFNGYim8AVHJU0z4ebYZW4JwAWSUtVVp1oQAAAGsAAAJRAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAGgRhAAAAAGZLKQsAAAAAAAAAAAAc0RsAAAAAAAAAACrpocAdnax31o9tGyySuVcwtYjCqzhK/YGT5XC4hH73AAAAF0h26AAAHNEbAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "ledger": 1888539,
                "createdAt": 1717166042
              },
              {
                "status": "SUCCESS",
                "applicationOrder": 3,
                "feeBump": false,
                "envelopeXdr": "AAAAAgAAAACwtG/IRC5DZE1UdekijEsoQEPM/uOwZ3iY/Y8UZ3b9xAAPQkAAGgRHAAAANgAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAABAAAAABB90WssODNIgi6BHveqzxTRmIpvAFRyVNM+Hm2GVuCcAAAAAAAAAADgdupKeB04lazKXCOb+E1JfxaM3tI4Xsb/qDa1MWOvXgAAABdIdugAAAAAAAAAAAJndv3EAAAAQKcTimw6KKcM0AeCMxXJcEK/hS9ROoj/qpMFppGNAr4W3ifSOSTGAFbA+cIVHmaV4p7xGcR+9JnUN1YjamvJZwSGVuCcAAAAQK9Cp775JbnYA793SXkkWWbmvnEFTiDPiFyTHxTphCwBDB1zqkXqGG6Q5O3dAyqkNJvj1XNRDsmY4pKV41qijQU=",
                "resultXdr": "AAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAA=",
                "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAc0RsAAAAAAAAAALC0b8hELkNkTVR16SKMSyhAQ8z+47BneJj9jxRndv3EAAAAADwzS2gAGgRHAAAANQAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAABzPVAAAAABmWdZ2AAAAAAAAAAEAHNEbAAAAAAAAAACwtG/IRC5DZE1UdekijEsoQEPM/uOwZ3iY/Y8UZ3b9xAAAAAA8M0toABoERwAAADYAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAc0RsAAAAAZlnf2gAAAAAAAAABAAAAAwAAAAMAHNEbAAAAAAAAAAAQfdFrLDgzSIIugR73qs8U0ZiKbwBUclTTPh5thlbgnABZJS1VWnWhAAAAawAAAlEAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAaBGEAAAAAZkspCwAAAAAAAAABABzRGwAAAAAAAAAAEH3Rayw4M0iCLoEe96rPFNGYim8AVHJU0z4ebYZW4JwAWSUWDOONoQAAAGsAAAJRAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAwAAAAAAGgRhAAAAAGZLKQsAAAAAAAAAAAAc0RsAAAAAAAAAAOB26kp4HTiVrMpcI5v4TUl/Foze0jhexv+oNrUxY69eAAAAF0h26AAAHNEbAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "ledger": 1888539,
                "createdAt": 1717166042
              },
              {
                "status": "SUCCESS",
                "applicationOrder": 4,
                "feeBump": false,
                "envelopeXdr": "AAAAAgAAAACxMt2gKYOehEoVbmh9vfvZ4mVzXFSNTbAU5S4a8zorrAA4wrwAHLqRAAAADAAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAQAAAAAAAAAAAAAAALEy3aApg56EShVuaH29+9niZXNcVI1NsBTlLhrzOiusz3K+BVgRzXig/Bhz1TL5Qy+Ibv6cDvCfdaAtBMMFPcYAAAAAHXUVmJM11pdJSKKV52UJrVYlvxaPLmmg17nMe0HGy0MAAAABAAAAAAAAAAEAAAAAAAAAAAAAAACxMt2gKYOehEoVbmh9vfvZ4mVzXFSNTbAU5S4a8zorrM9yvgVYEc14oPwYc9Uy+UMviG7+nA7wn3WgLQTDBT3GAAAAAB11FZiTNdaXSUiiledlCa1WJb8Wjy5poNe5zHtBxstDAAAAAAAAAAEAAAAAAAAAAQAAAAcddRWYkzXWl0lIopXnZQmtViW/Fo8uaaDXucx7QcbLQwAAAAEAAAAGAAAAAbolCtTsMrJvK0M2SaskFsaMajj3iAZbXxELZHwDyE5dAAAAFAAAAAEABf2jAAAd1AAAAGgAAAAAADjCWAAAAAHzOiusAAAAQM+qaiMKxMoCVNjdRIh3X9CSxkjAm0BpXYDB9Fd+DS0guYKiY3TMaVe243UB008iBn5ynQv724rReXlg7iFqXQA=",
                "resultXdr": "AAAAAAAw3cUAAAAAAAAAAQAAAAAAAAAYAAAAAKg/pGuhtOG27rIpG8xhUIp46CStGWOcsGlNsTQv44UOAAAAAA==",
                "resultMetaXdr": "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",
                "diagnosticEventsXdr": [
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACnJlYWRfZW50cnkAAAAAAAUAAAAAAAAAAg==",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAC3dyaXRlX2VudHJ5AAAAAAUAAAAAAAAAAQ==",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEGxlZGdlcl9yZWFkX2J5dGUAAAAFAAAAAAAAHdQ=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEWxlZGdlcl93cml0ZV9ieXRlAAAAAAAABQAAAAAAAABo",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADXJlYWRfa2V5X2J5dGUAAAAAAAAFAAAAAAAAAFQ=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADndyaXRlX2tleV9ieXRlAAAAAAAFAAAAAAAAAAA=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADnJlYWRfZGF0YV9ieXRlAAAAAAAFAAAAAAAAAAA=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD3dyaXRlX2RhdGFfYnl0ZQAAAAAFAAAAAAAAAGg=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAADnJlYWRfY29kZV9ieXRlAAAAAAAFAAAAAAAAHdQ=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD3dyaXRlX2NvZGVfYnl0ZQAAAAAFAAAAAAAAAAA=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACmVtaXRfZXZlbnQAAAAAAAUAAAAAAAAAAA==",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD2VtaXRfZXZlbnRfYnl0ZQAAAAAFAAAAAAAAAAA=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACGNwdV9pbnNuAAAABQAAAAAABTO4",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAACG1lbV9ieXRlAAAABQAAAAAAAPkD",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEWludm9rZV90aW1lX25zZWNzAAAAAAAABQAAAAAAAmiz",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAD21heF9yd19rZXlfYnl0ZQAAAAAFAAAAAAAAADA=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEG1heF9yd19kYXRhX2J5dGUAAAAFAAAAAAAAAGg=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAEG1heF9yd19jb2RlX2J5dGUAAAAFAAAAAAAAHdQ=",
                  "AAAAAAAAAAAAAAAAAAAAAgAAAAAAAAACAAAADwAAAAxjb3JlX21ldHJpY3MAAAAPAAAAE21heF9lbWl0X2V2ZW50X2J5dGUAAAAABQAAAAAAAAAA"
                ],
                "ledger": 1888539,
                "createdAt": 1717166042
              },
              {
                "status": "FAILED",
                "applicationOrder": 1,
                "feeBump": false,
                "envelopeXdr": "AAAAAgAAAAAxLMEcxmfUgNzL687Js4sX/jmFQDqTo1Lj4KDoC1PeSQAehIAAAAIJAAtMUQAAAAEAAAAAAAAAAAAAAABmWeAVAAAAAQAAAAlwc3BiOjMyMTcAAAAAAAACAAAAAQAAAACKlutUN5GT3UOoE2BUkNtJEwoipGOinBFsQtXgpIZMxQAAAAEAAAAA433o+yremWU3t88cKpfpHR+JMFR44JHzmBGni6hqCEYAAAACQVRVQUgAAAAAAAAAAAAAAGfK1mN4mg51jbX6by6TWghGynQ463doEDgzriqZo9bzAAAAAAaOd4AAAAABAAAAAIqW61Q3kZPdQ6gTYFSQ20kTCiKkY6KcEWxC1eCkhkzFAAAAAQAAAADjfej7Kt6ZZTe3zxwql+kdH4kwVHjgkfOYEaeLqGoIRgAAAAJBVFVTRAAAAAAAAAAAAAAAZ8rWY3iaDnWNtfpvLpNaCEbKdDjrd2gQODOuKpmj1vMAAAAAADh1IAAAAAAAAAACC1PeSQAAAEBoad/kqj/4Sqq5tC6HyeMm5LJKM1VqKRGZc3e4uvA3ITThwn2nNMRJRegdQrLrPBTSgw51nY8npilXVIds7I0OpIZMxQAAAEDTZNaLjIDMWPDdCxa1ZB28vUxTcS/0xykOFTI/JAz096vX6Y7wI0QvnbPM7KCoL0cJAciD+pJxNqXQ2Aff1hoO",
                "resultXdr": "AAAAAAAAAMj/////AAAAAgAAAAAAAAAB////+wAAAAAAAAAB////+wAAAAA=",
                "resultMetaXdr": "AAAAAwAAAAAAAAACAAAAAwAc0RwAAAAAAAAAADEswRzGZ9SA3Mvrzsmzixf+OYVAOpOjUuPgoOgLU95JAAAAFxzxIbUAAAIJAAtMUAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAMAAAAAABzRGgAAAABmWd/VAAAAAAAAAAEAHNEcAAAAAAAAAAAxLMEcxmfUgNzL687Js4sX/jmFQDqTo1Lj4KDoC1PeSQAAABcc8SG1AAACCQALTFEAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAADAAAAAAAc0RwAAAAAZlnf3wAAAAAAAAAAAAAAAAAAAAA=",
                "ledger": 1888540,
                "createdAt": 1717166047
              }
            ],
            "latestLedger": 1888542,
            "latestLedgerCloseTimestamp": 1717166057,
            "oldestLedger": 1871263,
            "oldestLedgerCloseTimestamp": 1717075350,
            "cursor": "8111217537191937"
          }
        }
            );

        let (s, _m) = get_mocked_server(request, response).await;
        let response = s
            .get_transactions(Pagination::From(1888539), 5)
            .await
            .unwrap();
        assert_eq!(response.latest_ledger, 1888542);
        assert_eq!(response.latest_ledger_close_timestamp, 1717166057);
        assert_eq!(response.oldest_ledger, 1871263);
        assert_eq!(response.oldest_ledger_close_timestamp, 1717075350);
        assert_eq!(response.cursor, "8111217537191937");
        assert_eq!(response.transactions.len(), 5);

        let ts = response.transactions;
        assert_eq!(ts[0].status, TransactionStatus::Failed);
        assert_eq!(ts[0].application_order, Some(1));
        assert_eq!(ts[0].fee_bump, Some(false));
        assert_eq!(ts[0].ledger, Some(1888539));
        assert_eq!(ts[0].created_at, Some(1717166042));
        if let stellar_baselib::xdr::TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: _,
        }) = ts[0].to_envelope().unwrap()
        {
            if let stellar_baselib::xdr::Memo::Text(text) = tx.memo {
                assert_eq!(text, "wallet:1712906336510".try_into().unwrap());
            } else {
                panic!("No memo")
            }
        } else {
            panic!("No enveloppe")
        }
        assert_eq!(ts[0].to_diagnostic_events(), None);

        assert_eq!(ts[3].status, TransactionStatus::Success);
        assert_eq!(ts[3].application_order, Some(4));
        assert_eq!(ts[3].fee_bump, Some(false));
        assert_eq!(ts[3].ledger, Some(1888539));
        assert_eq!(ts[3].created_at, Some(1717166042));
        let events = ts[3].to_diagnostic_events().unwrap();
        assert_eq!(events.len(), 19);

        let ContractEvent {
            ext: _,
            contract_id: _,
            type_,
            body,
        } = events[0].event.clone();

        let ContractEventBody::V0(ContractEventV0 { topics, data }) = body;
        //
        assert!(matches!(type_, ContractEventType::Diagnostic));
        assert_eq!(
            topics,
            vec![
                ScVal::Symbol(ScSymbol("core_metrics".try_into().unwrap())),
                ScVal::Symbol(ScSymbol("read_entry".try_into().unwrap()))
            ]
            .try_into()
            .unwrap()
        );
        assert_eq!(data, ScVal::U64(2));
    }
}
#[tokio::test]
async fn get_ledgers() {
    //
    let request = json!(
    {
      "jsonrpc": "2.0",
      "id": 1,
      "method": "getLedgers",
      "params": {
        "startLedger": 36233,
        "pagination": {
          "limit": 2
        }
      }
    }
                );
    let response = json!(
    {
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "ledgers": [
          {
            "hash": "434de11b427aa4b6f8cda259ac2111a6aa148d2ab6b4c7affe864e94a9f4bd80",
            "sequence": 36233,
            "ledgerCloseTime": "1734032457",
            "headerXdr": "Q03hG0J6pLb4zaJZrCERpqoUjSq2tMev/oZOlKn0vYAAAAAWY6AkQmSfV+Rrnpq308LrTR1IZq7dUQr4HkNPlpDpw4/4WH/HGddpIrl1KZiCUKzFOi9sqkX+b56aTGNqhTRHtwAAAABnWzxJAAAAAAAAAAEAAAAAqCTNGLyddQZNKZpbW6ykO8OqLzJpOBU9jC+btctt8DMAAABAjksBzacOp2IR06utjCmZ0QR6kgidREHCceghZapmZdNrS1uBiBG8Zz5Eh7uxMga5IAHPVbkSgFgXzjwRk4i2At8/YZgEqS/bQFcZLcQ910jqd4rcUrxJjOgFJMAUuBEZkxHVMCvc1iIZj7nTD2d91KT24H9FpOIGJLSiTuwys+0AAI2JDeC2s6dkAAAAAAAIewDXaQAAAAAAAAAAAAAB3wAAAGQATEtAAAAAyESlcNBlpidDCvLHH8Nu1MWDHWwUXTfbvGKqvbvlzzrb2GJe+2UfU8rr8mWrJXRTcQhl5UYgKhOQUOWP1nWgbp4AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "metadataXdr": "AAAAAQAAAABDTeEbQnqktvjNolmsIRGmqhSNKra0x6/+hk6UqfS9gAAAABZjoCRCZJ9X5GuemrfTwutNHUhmrt1RCvgeQ0+WkOnDj/hYf8cZ12kiuXUpmIJQrMU6L2yqRf5vnppMY2qFNEe3AAAAAGdbPEkAAAAAAAAAAQAAAACoJM0YvJ11Bk0pmltbrKQ7w6ovMmk4FT2ML5u1y23wMwAAAECOSwHNpw6nYhHTq62MKZnRBHqSCJ1EQcJx6CFlqmZl02tLW4GIEbxnPkSHu7EyBrkgAc9VuRKAWBfOPBGTiLYC3z9hmASpL9tAVxktxD3XSOp3itxSvEmM6AUkwBS4ERmTEdUwK9zWIhmPudMPZ33UpPbgf0Wk4gYktKJO7DKz7QAAjYkN4Lazp2QAAAAAAAh7ANdpAAAAAAAAAAAAAAHfAAAAZABMS0AAAADIRKVw0GWmJ0MK8scfw27UxYMdbBRdN9u8Yqq9u+XPOtvYYl77ZR9TyuvyZasldFNxCGXlRiAqE5BQ5Y/WdaBungAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFjoCRCZJ9X5GuemrfTwutNHUhmrt1RCvgeQ0+WkOnDjwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADVBewAAAAAAAAAAA=="
          },
          {
            "hash": "201f38ca43cfad09d00287ee70d77edc8c88a61e018683fd466e71688222a217",
            "sequence": 36234,
            "ledgerCloseTime": "1734032462",
            "headerXdr": "IB84ykPPrQnQAofucNd+3IyIph4BhoP9Rm5xaIIiohcAAAAWQ03hG0J6pLb4zaJZrCERpqoUjSq2tMev/oZOlKn0vYB8UE9ZJ4uLDnIu/g7+08D/eYnJjrJD07fGpWaVoLi29wAAAABnWzxOAAAAAAAAAAEAAAAAtV0Qsrt9KE7VwDpHpeG1kMkPV4NVRxzEUHu+ggTSPIsAAABAN6Cc3KxXPM2WmFDtY/yPZM0aha2yhnHFBDaliMqO6BzVQMb9Zlq5DoGrLr/xoFOp8YJ3RjLedF8w/KfN3ogPBuzRjPABveSt1CvvmDdnwUpSXyUsjdjECdbpjuKxaZZR+NeVPfeYT06qs2v4JwBff4WWpIGwEXPcxq3tZKuekH8AAI2KDeC2s6dkAAAAAAAIewDaiQAAAAAAAAAAAAAB3wAAAGQATEtAAAAAyESlcNBlpidDCvLHH8Nu1MWDHWwUXTfbvGKqvbvlzzrb2GJe+2UfU8rr8mWrJXRTcQhl5UYgKhOQUOWP1nWgbp4AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "metadataXdr": "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"
          }
        ],
        "latestLedger": 36379,
        "latestLedgerCloseTime": 1734033188,
        "oldestLedger": 29312,
        "oldestLedgerCloseTime": 1733997822,
        "cursor": "36234"
      }
    }
                );

    let (s, _m) = get_mocked_server(request, response).await;
    let response = s.get_ledgers(Pagination::From(36233), 2).await.unwrap();

    assert_eq!(response.latest_ledger, 36379);
    assert_eq!(response.latest_ledger_close_time, 1734033188);
    assert_eq!(response.oldest_ledger, 29312);
    assert_eq!(response.oldest_ledger_close_time, 1733997822);
    assert_eq!(response.ledgers.len(), 2);

    let l1 = response.ledgers[0].clone();
    let hash1 = "434de11b427aa4b6f8cda259ac2111a6aa148d2ab6b4c7affe864e94a9f4bd80";
    assert_eq!(l1.hash, hash1);
    assert_eq!(l1.sequence, 36233);
    assert_eq!(l1.ledger_close_time, "1734032457");

    if let Some(LedgerHeaderHistoryEntry {
        hash,
        header,
        ext: _,
    }) = l1.to_header()
    {
        assert_eq!(hash.0, hex::decode(hash1).unwrap().as_slice());
        assert_eq!(header.ledger_seq, 36233);
    } else {
        panic!("No header")
    }
    if let Some(LedgerCloseMeta::V1(LedgerCloseMetaV1 {
        ext: _,
        ledger_header: _,
        tx_set,
        tx_processing: _,
        upgrades_processing: _,
        scp_info: _,
        total_byte_size_of_live_soroban_state,
        evicted_keys: _,
        unused: _,
    })) = l1.to_metadata()
    {
        let GeneralizedTransactionSet::V1(TransactionSetV1 {
            previous_ledger_hash,
            phases: _,
        }) = tx_set;
        assert_eq!(
            previous_ledger_hash.0,
            hex::decode("63a02442649f57e46b9e9ab7d3c2eb4d1d4866aedd510af81e434f9690e9c38f")
                .unwrap()
                .as_slice()
        );
        assert_eq!(total_byte_size_of_live_soroban_state, 13960684);
    } else {
        panic!("No metadata")
    }

    let l2 = response.ledgers[1].clone();
    let hash2 = "201f38ca43cfad09d00287ee70d77edc8c88a61e018683fd466e71688222a217";
    assert_eq!(l2.hash, hash2);
    assert_eq!(l2.sequence, 36234);
    assert_eq!(l2.ledger_close_time, "1734032462");

    if let Some(LedgerHeaderHistoryEntry {
        hash,
        header,
        ext: _,
    }) = l2.to_header()
    {
        assert_eq!(hash.0, hex::decode(hash2).unwrap().as_slice());
        assert_eq!(header.ledger_seq, 36234);
    } else {
        panic!("No header")
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
            allow_http: true,
            ..Default::default()
        },
    )
    .expect("Configuration should not fail");

    (server, mock_server)
}
async fn get_friendbot_server(account_id: &str, response: serde_json::Value) -> MockServer {
    let mock_server = MockServer::start().await;

    let response = ResponseTemplate::new(200).set_body_json(response);
    Mock::given(method("GET"))
        .and(path("/"))
        .and(query_param("addr", account_id))
        .respond_with(response)
        .expect(1..)
        .mount(&mock_server)
        .await;

    mock_server
}

#[tokio::test]
async fn native_check_balance_testnet() {
    let rpc = Server::new("https://soroban-testnet.stellar.org", Options::default()).unwrap();

    let native_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    let native_sac = Contracts::new(native_id).unwrap();

    let kp = Keypair::random().unwrap();
    let account = rpc.request_airdrop(&kp.public_key()).await.unwrap();

    let source_account = Rc::new(RefCell::new(
        Account::new(&kp.public_key(), &account.sequence_number()).unwrap(),
    ));

    let account_address = Address::new(&kp.public_key()).unwrap();
    let tx = TransactionBuilder::new(source_account, Networks::testnet(), None)
        .fee(1000u32)
        .add_operation(native_sac.call("balance", Some(vec![account_address.to_sc_val().unwrap()])))
        .build();

    let response = rpc.simulate_transaction(&tx, None).await.unwrap();
    if let Some((ScVal::I128(Int128Parts { hi, lo }), _auth)) = response.to_result() {
        let balance = i128_from_pieces(hi, lo) / 10000000; // Divide to convert from stroops to XLM
        println!("Account {} has {} XLM", kp.public_key(), balance);
        assert_eq!(balance, 10000);
    } else {
        panic!("Failed")
    }
}
#[tokio::test]
async fn native_events_testnet() {
    let rpc = Server::new(
        "https://soroban-testnet.stellar.org",
        Options {
            timeout: 30,
            ..Default::default()
        },
    )
    .unwrap();

    let native_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let response = rpc.get_latest_ledger().await.unwrap();
    let ledger = response.sequence;

    let transfer = ScVal::Symbol(ScSymbol("transfer".try_into().unwrap()));
    let native = ScVal::String(ScString("native".try_into().unwrap()));
    let events = rpc
        .get_events(
            crate::Pagination::From(ledger - 10),
            vec![EventFilter::new(crate::soroban_rpc::EventType::All)
                .contract(native_id)
                .topic(vec![
                    Topic::Val(transfer),
                    Topic::Any, // From account
                    Topic::Any, // To account
                    Topic::Val(native),
                ])],
            Some(3),
        )
        .await
        .unwrap();

    println!("{:?}", events);
}
