/// Error Handling in `soroban_client` crate
/// This module defines all possible error types used in the `soroban_client` crate.
use stellar_baselib::xdr::SorobanTransactionData;
use thiserror::Error;

/// Possible error types
#[derive(Error, Debug)]
pub enum Error {
    /// Error for invalid RPC URL
    #[error(transparent)]
    InvalidRpc(#[from] InvalidRpcUrl),
    /// Error when XDR processing fails
    #[error("XdrError")]
    XdrError,
    /// Error when JSON parsing fails, with a descriptive message
    #[error("JsonError: could not parse {0}")]
    JsonError(String),
    /// Error for network-related failures
    #[error("NetworkError")]
    NetworkError(#[from] reqwest::Error),
    /// Error when an account is not found
    #[error("AccountError")]
    AccountNotFound,
    /// Error when contract data is missing
    #[error("ContractError")]
    ContractDataNotFound,
    /// Error for general transaction failures
    #[error("TransactionError")]
    TransactionError,
    /// Error for invalid Soroban transactions
    #[error("InvalidSorobanTransaction")]
    InvalidSorobanTransaction,
    /// Error when a simulation fails
    #[error("SimulationFailed")]
    SimulationFailed,
    /// Error when restoration is required with additional data
    #[error("RestorationRequired")]
    RestorationRequired(i64, SorobanTransactionData),
    /// Error for RPC failures, includes code and message
    #[error("RPCError {code}: {message}")]
    RPCError {
        /// The error code returned from the RPC
        code: i32,
        /// The error message returned from the RPC
        message: String,
    },
    /// Unexpected error, should be reported
    #[error("UnexpectedError")]
    UnexpectedError,
    /// Error when Friendbot is not available on the current network
    #[error("NoFriendbot")]
    NoFriendbot,
}

/// Possible  errors for invalid RPC URLs
#[derive(Error, Debug)]
pub enum InvalidRpcUrl {
    /// Error when the URL scheme is not HTTP or HTTPS
    #[error("The RPC Url scheme should be http or https")]
    NotHttpScheme,
    /// Error when insecure HTTP URLs are used without explicit permission
    #[error("Http scheme requires the option allow_http: true")]
    UnsecureHttpNotAllowed,
    /// Error when the provided URL is invalid
    #[error("InvalidUrl")]
    InvalidUri,
}
