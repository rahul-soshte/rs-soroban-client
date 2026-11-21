use serde_json::json;
use soroban_client::{
    account::{Account, AccountBehavior},
    asset::{Asset, AssetBehavior},
    error::Error,
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    soroban_rpc::SendTransactionStatus,
    transaction::{
        FeeBumpError, FeeBumpTransaction, ParsedTransaction, Transaction, TransactionBehavior,
        TransactionBuilder, TransactionBuilderBehavior, TransactionEnvelopeSource,
    },
    xdr::{self, Limits, ReadXdr, WriteXdr},
    Options, Server,
};
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

fn signed_transactions() -> (Transaction, FeeBumpTransaction) {
    let source = Keypair::from_raw_ed25519_seed(&[10; 32]).unwrap();
    let payer = Keypair::from_raw_ed25519_seed(&[20; 32]).unwrap();
    assert_ne!(source.public_key(), payer.public_key());
    let mut account = Account::new(&source.public_key(), "42").unwrap();
    let mut inner = TransactionBuilder::new(&mut account, Networks::testnet(), None)
        .fee(100_u32)
        .add_operation(
            Operation::new()
                .payment(&payer.public_key(), &Asset::native(), 1)
                .unwrap(),
        )
        .build();
    inner.sign(&[source]);
    let mut bump = FeeBumpTransaction::new(
        &payer.public_key(),
        i64::from(u32::MAX) + 1,
        inner.to_envelope().unwrap(),
        Networks::testnet(),
        100,
    )
    .unwrap();
    bump.sign(&[payer]).unwrap();
    (inner, bump)
}

fn server(mock: &MockServer) -> Server {
    Server::new(
        &mock.uri(),
        Options {
            allow_http: true,
            ..Default::default()
        },
    )
    .unwrap()
}

async fn assert_submission(
    transaction: impl TransactionEnvelopeSource,
    expected: xdr::TransactionEnvelope,
    hash: &str,
    status: SendTransactionStatus,
) {
    let mock = MockServer::start().await;
    let expected_xdr = expected.to_xdr_base64(Limits::none()).unwrap();
    let error = (status == SendTransactionStatus::Error).then_some(xdr::TransactionResult {
        fee_charged: 0,
        result: xdr::TransactionResultResult::TxInsufficientFee,
        ext: xdr::TransactionResultExt::V0,
    });
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(json!({
            "jsonrpc": "2.0", "id": 1, "method": "sendTransaction",
            "params": { "transaction": expected_xdr },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "status": status,
                "hash": hash,
                "latestLedger": 123,
                "latestLedgerCloseTime": "1700159337",
                "errorResultXdr": error.as_ref().map(|result| result.to_xdr_base64(Limits::none()).unwrap()),
            },
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let result = server(&mock).send_transaction(transaction).await.unwrap();
    assert_eq!(result.status, status);
    assert_eq!(result.hash, hash);
    assert_eq!(result.latest_ledger, 123);
    assert_eq!(result.latest_ledger_close_time, "1700159337");
    assert_eq!(result.to_error_result(), error);
    let requests = mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let actual = xdr::TransactionEnvelope::from_xdr_base64(
        request["params"]["transaction"].as_str().unwrap(),
        Limits::none(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn submits_owned_and_borrowed_ordinary_and_parsed_transactions() {
    let (inner, _) = signed_transactions();
    let expected = inner.to_envelope().unwrap();
    let hash = hex::encode(inner.hash());
    let parsed = Transaction::from_xdr_envelope(
        &expected.to_xdr_base64(Limits::none()).unwrap(),
        Networks::testnet(),
    )
    .unwrap();
    assert_submission(
        inner.clone(),
        expected.clone(),
        &hash,
        SendTransactionStatus::Pending,
    )
    .await;
    assert_submission(
        &inner,
        expected.clone(),
        &hash,
        SendTransactionStatus::Pending,
    )
    .await;
    assert_submission(
        parsed.clone(),
        expected.clone(),
        &hash,
        SendTransactionStatus::Pending,
    )
    .await;
    assert_submission(&parsed, expected, &hash, SendTransactionStatus::Pending).await;
}

#[tokio::test]
async fn submits_complete_fee_bump_for_owned_borrowed_and_parsed_inputs() {
    let (inner, bump) = signed_transactions();
    let expected = bump.to_envelope();
    let hash = hex::encode(bump.hash().unwrap());
    assert_ne!(hash, hex::encode(inner.hash()));
    assert_eq!(bump.inner_hash().unwrap(), inner.hash());
    assert_eq!(bump.inner_envelope().signatures.to_vec(), inner.signatures);
    assert_eq!(bump.envelope().signatures.len(), 1);
    assert!(bump.envelope().tx.fee > i64::from(u32::MAX));
    let parsed = Transaction::from_xdr_envelope(
        &expected.to_xdr_base64(Limits::none()).unwrap(),
        Networks::testnet(),
    )
    .unwrap();
    assert_submission(
        bump.clone(),
        expected.clone(),
        &hash,
        SendTransactionStatus::Pending,
    )
    .await;
    assert_submission(
        &bump,
        expected.clone(),
        &hash,
        SendTransactionStatus::Duplicate,
    )
    .await;
    assert_submission(
        parsed.clone(),
        expected.clone(),
        &hash,
        SendTransactionStatus::TryAgainLater,
    )
    .await;
    assert_submission(&parsed, expected, &hash, SendTransactionStatus::Error).await;
}

#[tokio::test]
async fn submits_v0_transactions_without_changing_the_envelope_variant() {
    let (inner, _) = signed_transactions();
    let xdr::TransactionEnvelope::Tx(v1) = inner.to_envelope().unwrap() else {
        panic!("expected V1");
    };
    let xdr::MuxedAccount::Ed25519(source) = v1.tx.source_account else {
        panic!("expected an Ed25519 source");
    };
    let expected = xdr::TransactionEnvelope::TxV0(xdr::TransactionV0Envelope {
        tx: xdr::TransactionV0 {
            source_account_ed25519: source,
            fee: v1.tx.fee,
            seq_num: v1.tx.seq_num,
            time_bounds: None,
            memo: v1.tx.memo,
            operations: v1.tx.operations,
            ext: xdr::TransactionV0Ext::V0,
        },
        signatures: v1.signatures,
    });
    let parsed = Transaction::from_xdr_envelope(
        &expected.to_xdr_base64(Limits::none()).unwrap(),
        Networks::testnet(),
    )
    .unwrap();
    let ordinary = parsed.clone().into_transaction().unwrap();
    let hash = hex::encode(inner.hash());
    assert_eq!(hex::encode(ordinary.hash()), hash);
    assert_submission(
        ordinary,
        expected.clone(),
        &hash,
        SendTransactionStatus::Pending,
    )
    .await;
    assert_submission(parsed, expected, &hash, SendTransactionStatus::Pending).await;
}

#[tokio::test]
async fn submits_fee_bump_with_soroban_data_and_v2_preconditions_intact() {
    let (mut inner, _) = signed_transactions();
    inner.operations = Some(vec![xdr::Operation {
        source_account: None,
        body: xdr::OperationBody::RestoreFootprint(xdr::RestoreFootprintOp::default()),
    }]);
    let resource_fee = i64::from(u32::MAX) + 1_000;
    inner.soroban_data = Some(xdr::SorobanTransactionData {
        resource_fee,
        ..Default::default()
    });
    inner.min_account_sequence = Some("42".into());
    inner.min_account_sequence_age = Some(10);
    inner.signatures.clear();
    inner.sign(&[Keypair::from_raw_ed25519_seed(&[10; 32]).unwrap()]);
    let payer = Keypair::from_raw_ed25519_seed(&[20; 32]).unwrap();
    let mut bump = FeeBumpTransaction::new(
        &payer.public_key(),
        resource_fee + 200,
        inner.to_envelope().unwrap(),
        Networks::testnet(),
        100,
    )
    .unwrap();
    bump.sign(&[payer]).unwrap();
    let expected = bump.to_envelope();
    let hash = hex::encode(bump.hash().unwrap());
    assert_submission(&bump, expected, &hash, SendTransactionStatus::Pending).await;
    assert_eq!(bump.inner_hash().unwrap(), inner.hash());
}

#[tokio::test]
async fn rejects_invalid_transaction_conversion_before_sending_a_request() {
    let mock = MockServer::start().await;
    let rpc = server(&mock);
    let (inner, _) = signed_transactions();
    let mut missing_source = inner.clone();
    missing_source.source = None;
    let mut too_many_signatures = inner.clone();
    too_many_signatures.signatures = vec![inner.signatures[0].clone(); 21];
    let mut unsupported = inner;
    unsupported.envelope_type = xdr::EnvelopeType::TxFeeBump;
    for invalid in [missing_source, too_many_signatures, unsupported] {
        assert!(matches!(
            rpc.send_transaction(&invalid).await,
            Err(Error::TransactionError)
        ));
        assert!(matches!(
            rpc.send_transaction(ParsedTransaction::Transaction(invalid))
                .await,
            Err(Error::TransactionError)
        ));
    }
    assert!(mock.received_requests().await.unwrap().is_empty());
}

#[test]
fn exposes_typed_fee_construction_errors_through_the_client() {
    let (inner, _) = signed_transactions();
    let payer = Keypair::from_raw_ed25519_seed(&[20; 32]).unwrap();
    assert!(matches!(
        FeeBumpTransaction::new(
            &payer.public_key(),
            199,
            inner.to_envelope().unwrap(),
            Networks::testnet(),
            100,
        ),
        Err(FeeBumpError::InsufficientFee {
            fee: 199,
            minimum: 200
        })
    ));
}
