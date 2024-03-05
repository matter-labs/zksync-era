//! Backend "glue" which ties the actual Web3 API implementation to the `jsonrpsee` JSON RPC backend.
//! Consists mostly of boilerplate code implementing the `jsonrpsee` server traits for the corresponding
//! namespace structures defined in `zksync_core`.

use std::fmt;

use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned},
};

use crate::api_server::{tx_sender::SubmitTxError, web3::metrics::API_METRICS};

pub mod batch_limiter_middleware;
pub mod namespaces;

pub(crate) fn into_jsrpc_error(err: Web3Error) -> ErrorObjectOwned {
    let data = match &err {
        Web3Error::SubmitTransactionError(_, data) => Some(format!("0x{}", hex::encode(data))),
        _ => None,
    };
    ErrorObjectOwned::owned(
        match err {
            Web3Error::InternalError | Web3Error::NotImplemented => ErrorCode::InternalError.code(),
            Web3Error::NoBlock
            | Web3Error::PrunedBlock(_)
            | Web3Error::PrunedL1Batch(_)
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
            Web3Error::SubmitTransactionError(message, _) => message,
            _ => err.to_string(),
        },
        data,
    )
}

impl SubmitTxError {
    /// Maps this error into [`Web3Error`]. If this is an internal error, error details are logged, but are not returned
    /// to the client.
    pub(crate) fn into_web3_error(self, method_name: &'static str) -> Web3Error {
        match self {
            Self::Internal(err) => internal_error(method_name, err),
            Self::ProxyError(ref err) => {
                // Strip internal error details that should not be exposed to the caller.
                tracing::warn!("Error proxying call to main node in method {method_name}: {err}");
                Web3Error::SubmitTransactionError(err.as_ref().to_string(), self.data())
            }
            _ => Web3Error::SubmitTransactionError(self.to_string(), self.data()),
        }
    }
}

pub(crate) fn internal_error(method_name: &'static str, error: impl fmt::Display) -> Web3Error {
    tracing::error!("Internal error in method {method_name}: {error}");
    API_METRICS.web3_internal_errors[&method_name].inc();
    Web3Error::InternalError
}
