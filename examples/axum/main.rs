use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use soroban_client::{
    keypair::{Keypair, KeypairBehavior},
    xdr, Options, Server,
};

type SharedState = Arc<Server>;

#[tokio::main]
async fn main() {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = Server::new(server_url, Options::default()).expect("Cannot create server");
    let state = Arc::new(server);

    let app = Router::new()
        .route("/", get(get_version_info))
        .route("/account/{id}", get(get_account))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("Listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

async fn get_version_info(State(server): State<SharedState>) -> Json<Value> {
    let response = server.get_version_info().await;

    match response {
        Ok(info) => Json(json!({
            "version": info.version,
            "protocol": info.protocol_version
        })),
        Err(err) => Json(json!({
            "error": err.to_string()
        })),
    }
}

async fn get_account(State(server): State<SharedState>, Path(id): Path<String>) -> Json<Value> {
    let account_id = match Keypair::from_public_key(&id) {
        Ok(k) => k.xdr_account_id(),
        _ => {
            return Json(json!({
                "error": "Malformed account id"
            }));
        }
    };

    let ledger_key = xdr::LedgerKey::Account(xdr::LedgerKeyAccount { account_id });
    let response = server.get_ledger_entries(vec![ledger_key]).await;
    match response {
        Ok(valid) => {
            if let Some(entries) = valid.entries {
                if !entries.is_empty() {
                    if let xdr::LedgerEntryData::Account(account) = entries[0].to_data() {
                        Json(json!({
                            "accound_id": account.account_id,
                            "sequence": account.seq_num,
                            "balance": account.balance,
                        }))
                    } else {
                        Json(json!({
                            "error": "internal error"
                        }))
                    }
                } else {
                    Json(json!({
                        "error": "Account does not exist"
                    }))
                }
            } else {
                Json(json!({
                    "error": "Account does not exist"
                }))
            }
        }
        Err(err) => Json(json!({
            "error": err.to_string()
        })),
    }
}
