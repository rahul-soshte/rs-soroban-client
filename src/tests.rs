use crate::error::*;
use crate::server::*;
use crate::soroban_rpc::GetHealthResponse;
use crate::soroban_rpc::GetHealthWrapperResponse;
use crate::soroban_rpc::GetLatestLedgerResponse;
use crate::soroban_rpc::GetNetworkResponse;
use crate::soroban_rpc::GetNetworkResponseWrapper;
use serde_json::json;
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
