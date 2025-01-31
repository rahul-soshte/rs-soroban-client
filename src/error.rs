use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    InvalidRpc(#[from] InvalidRpcUrl),
    #[error("XdrError")]
    XdrError,
    #[error("NetworkError")]
    NetworkError(#[from] reqwest::Error),
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
    #[error("RestorationRequired")]
    RestorationRequired,
    #[error("RPCError {code}: {message}")]
    RPCError { code: i32, message: String },
    #[error("UnexpectedError: should not happen, please report a bug")]
    UnexpectedError,
}

#[derive(Error, Debug)]
pub enum InvalidRpcUrl {
    #[error("The RPC Url scheme should be http or https")]
    NotHttpScheme,
    #[error("Http scheme requires the option allow_http: true")]
    UnsecureHttpNotAllowed,
    #[error("InvalidUrl")]
    InvalidUri,
}
