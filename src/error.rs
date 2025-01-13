use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    InvalidRpc(#[from] InvalidRpcUrl),
    #[error("XdrError")]
    XdrError,
    #[error("NetworkError")]
    NetworkError,
    #[error("AccountError")]
    AccountNotFound,
    #[error("ContractError")]
    ContractDataNotFound,
    #[error("TransactionError")]
    TransactionError, // temporary
    #[error("InvalidSorobanTransaction: must contain exactly one invokeHostFunction, extendFootprintTtl, or restoreFootprint operation")]
    InvalidSorobanTransaction,
    #[error("SimulationFailed")]
    SimulationFailed,
}

#[derive(Error, Debug)]
pub enum InvalidRpcUrl {
    #[error("The RPC Url scheme should be http or https")]
    NotHttpScheme,
    #[error("Http scheme requires the option allow_http: true")]
    UnsecureHttpNotAllowed,
    #[error(transparent)]
    InvalidUri(#[from] http::uri::InvalidUri),
}
