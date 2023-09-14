use reqwest::{Client, header};

pub static VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn create_client() -> Client {
    let mut headers = header::HeaderMap::new();
    headers.insert("X-Client-Name", "rs-soroban-client".parse().unwrap());
    headers.insert("X-Client-Version", VERSION.parse().unwrap());

    Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

pub use create_client as HTTPClient;