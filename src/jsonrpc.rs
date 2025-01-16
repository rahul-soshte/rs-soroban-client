use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Str(String),
    Num(i64),
}

#[derive(Debug, Serialize)]
pub struct Request<T> {
    jsonrpc: String,
    id: i32,
    method: String,
    params: T,
}

#[derive(Debug, Serialize)]
pub struct Notification<T> {
    jsonrpc: &'static str,
    method: String,
    params: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct Response<T> {
    jsonrpc: String,
    id: i32,
    result: Option<T>,
    error: Option<Error>,
}

#[derive(Debug, Deserialize)]
pub struct Error {
    #[allow(dead_code)]
    code: i32,
    #[allow(dead_code)]
    message: Option<String>,
}

pub async fn post<T: for<'a> Deserialize<'a> + std::fmt::Debug>(
    url: &str,
    method: &str,
    params: HashMap<String, serde_json::Value>,
) -> Result<T, reqwest::Error> {
    let client = Client::new();
    let request = Request {
        jsonrpc: "2.0".into(),
        id: 1,
        method: method.to_string(),
        params,
    };

    let res = client.post(url).json(&request).send().await?;
    //dbg!(&res.text().await);
    //panic!();
    let data = res.json::<Response<T>>().await?;
    // println!("Data {:?}", data);
    // Here we ensure that the response status is not an error.
    // If it's an error, it will convert the response into an error type
    // let dc = res.error_for_status_ref()?;
    // TODO: Add Error Check

    let r = data.result;
    Ok(r.unwrap())
}

pub async fn post_object<T: for<'a> Deserialize<'a> + std::fmt::Debug>(
    url: &str,
    method: &str,
    param: serde_json::Value,
) -> Result<T, reqwest::Error> {
    post(url, method, hashmap! { "param".to_string() => param }).await
}
