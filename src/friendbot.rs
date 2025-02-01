use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FriendbotResponse {
    pub successful: Option<bool>,
}
