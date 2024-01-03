//! Backend "glue" which ties the actual Web3 API implementation to the `jsonrpsee` JSON RPC backend.
//! Consists mostly of boilerplate code implementing the `jsonrpsee` server traits for the corresponding
//! namespace structures defined in `zksync_core`.

use std::{error::Error, fmt};

use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned},
};

use crate::api_server::web3::metrics::API_METRICS;

pub mod batch_limiter_middleware;
pub mod namespaces;

pub fn from_std_error(e: impl Error) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(ErrorCode::InternalError.code(), e.to_string(), Some(()))
}

pub fn into_jsrpc_error(err: Web3Error) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(
        match err {
            Web3Error::InternalError | Web3Error::NotImplemented => ErrorCode::InternalError.code(),
            Web3Error::NoBlock
            | Web3Error::NoSuchFunction
            | Web3Error::RLPError(_)
            | Web3Error::InvalidTransactionData(_)
            | Web3Error::TooManyTopics
            | Web3Error::FilterNotFound
            | Web3Error::InvalidFeeParams(_)
            | Web3Error::InvalidFilterBlockHash
            | Web3Error::LogsLimitExceeded(_, _, _) => ErrorCode::InvalidParams.code(),
            Web3Error::SubmitTransactionError(_, _) | Web3Error::SerializationError(_) => 3,
            Web3Error::PubSubTimeout => 4,
            Web3Error::RequestTimeout => 5,
            Web3Error::TreeApiUnavailable => 6,
        },
        match err {
            Web3Error::SubmitTransactionError(ref message, _) => message.clone(),
            _ => err.to_string(),
        },
        match err {
            Web3Error::SubmitTransactionError(_, data) => Some(format!("0x{}", hex::encode(data))),
            _ => None,
        },
    )
}

pub fn internal_error(method_name: &'static str, error: impl fmt::Display) -> Web3Error {
    tracing::error!("Internal error in method {method_name}: {error}");
    API_METRICS.web3_internal_errors[&method_name].inc();
    Web3Error::InternalError
}
