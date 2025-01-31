use futures::TryFutureExt;
use http::{HeaderName, HeaderValue};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

use crate::error::Error::NetworkError;

#[derive(Debug)]
pub struct JsonRpc {
    client: reqwest::Client,
    server_url: reqwest::Url,
}

impl JsonRpc {
    pub fn new(server_url: reqwest::Url, timeout: u64, headers: HashMap<String, String>) -> Self {
        let mut http_headers = reqwest::header::HeaderMap::new();
        http_headers.insert(
            "X-Client-Name",
            HeaderValue::from_static("rs-soroban-client"),
        );
        http_headers.insert("X-Client-Version", HeaderValue::from_static(crate::VERSION));

        for (key, value) in headers {
            if let Ok(header_name) = HeaderName::try_from(key) {
                let header_value =
                    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(""));

                http_headers.insert(header_name, header_value);
            }
        }

        let client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(timeout))
            .default_headers(http_headers)
            .build()
            .expect("Cannot build http client");
        JsonRpc { client, server_url }
    }
    pub async fn post<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<Response<R>, crate::error::Error> {
        let url = self.server_url.clone();
        let method = method.to_string();
        let res = self
            .client
            .post(url)
            .json(&Request {
                jsonrpc: "2.0".to_string(),
                id: 1,
                method,
                params,
            })
            .send()
            .map_err(NetworkError)
            .await?;

        res.json().map_err(NetworkError).await
    }
}

#[derive(Debug, Serialize)]
pub struct Request<T> {
    jsonrpc: String,
    id: i32,
    method: String,
    params: T,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Response<T> {
    jsonrpc: String,
    id: i32,
    pub result: Option<T>,
    pub error: Option<Error>,
}

#[derive(Debug, Deserialize)]
pub struct Error {
    #[allow(dead_code)]
    pub code: i32,
    #[allow(dead_code)]
    pub message: Option<String>,
}
