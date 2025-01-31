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

#[cfg(test)]
mod test {

    use std::collections::HashMap;
    use std::str::FromStr;

    use reqwest::Url;
    use serde::Deserialize;
    use serde_json::json;
    use wiremock::matchers;
    use wiremock::matchers::headers;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;

    use crate::jsonrpc::JsonRpc;
    use crate::jsonrpc::Response;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Data {
        number: u64,
        string: String,
        vec: Vec<u8>,
    }

    #[tokio::test]
    async fn test() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "echo",
            "params": {
                "number": 3,
                "string": "a string",
                "vec": [1, 2, 3]
        }

        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": 3,
                "string": "a string",
                "vec": [1, 2, 3]
            }
        });
        let mock_server = MockServer::start().await;

        let response = ResponseTemplate::new(200).set_body_json(response);
        Mock::given(method("POST"))
            .and(path("/"))
            .and(headers("x-api-key", vec!["9864920430304"]))
            .and(matchers::body_partial_json(request))
            .respond_with(response)
            .expect(1..)
            .mount(&mock_server)
            .await;

        let server_url = Url::from_str(&mock_server.uri()).unwrap();
        let mut headers: HashMap<String, String> = HashMap::new();
        headers.insert("x-api-key".into(), "9864920430304".into());
        let rpc = JsonRpc::new(server_url, 10, headers);

        let params = json!({
                "number": 3,
                "string": "a string",
                "vec": [1, 2, 3]
        });

        let response: Response<Data> = rpc.post("echo", params).await.unwrap();
        assert_eq!(response.id, 1);
        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(
            response.result,
            Some(Data {
                number: 3,
                string: "a string".to_string(),
                vec: vec![1, 2, 3]
            })
        );
    }
}
