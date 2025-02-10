use stellar_baselib::xdr::SorobanTransactionData;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    InvalidRpc(#[from] InvalidRpcUrl),
    #[error("XdrError")]
    XdrError,
    #[error("JsonError: could not parse {0}")]
    JsonError(String),
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
    RestorationRequired(i64, SorobanTransactionData),
    #[error("RPCError {code}: {message}")]
    RPCError { code: i32, message: String },
    #[error("UnexpectedError: should not happen, please report a bug")]
    UnexpectedError,
    #[error("NoFriendbot: No friendbot on current network")]
    NoFriendbot,
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
