//! Backend "glue" which ties the actual Web3 API implementation to the `jsonrpsee` JSON RPC backend.
//! Consists mostly of boilerplate code implementing the `jsonrpsee` server traits for the corresponding
//! namespace structures defined in `zksync_core`.

use std::fmt;

use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned},
};

pub(crate) use self::middleware::{LimitMiddleware, MetadataMiddleware, MethodMetadata};
use crate::api_server::tx_sender::SubmitTxError;

mod middleware;
pub mod namespaces;

pub(crate) fn into_jsrpc_error(err: Web3Error) -> ErrorObjectOwned {
    MethodMetadata::with(|meta| {
        meta.set_error(&err);
    });

    let data = match &err {
        Web3Error::SubmitTransactionError(_, data) => Some(format!("0x{}", hex::encode(data))),
        _ => None,
    };
    let code = match err {
        Web3Error::InternalError => ErrorCode::InternalError.code(),
        Web3Error::NoBlock
        | Web3Error::PrunedBlock(_)
        | Web3Error::PrunedL1Batch(_)
        | Web3Error::TooManyTopics
        | Web3Error::FilterNotFound
        | Web3Error::InvalidFilterBlockHash
        | Web3Error::LogsLimitExceeded(_, _, _) => ErrorCode::InvalidParams.code(),
        Web3Error::SubmitTransactionError(_, _) | Web3Error::SerializationError(_) => 3,
        Web3Error::TreeApiUnavailable => 6,
    };
    let message = match err {
        Web3Error::SubmitTransactionError(message, _) => message,
        _ => err.to_string(),
    };

    ErrorObjectOwned::owned(code, message, data)
}

impl SubmitTxError {
    /// Maps this error into [`Web3Error`]. If this is an internal error, error details are logged, but are not returned
    /// to the client.
    pub(crate) fn into_web3_error(self) -> Web3Error {
        match self {
            Self::Internal(err) => internal_error(err),
            Self::ProxyError(ref err) => {
                // Strip internal error details that should not be exposed to the caller.
                MethodMetadata::with(|meta| {
                    tracing::warn!(
                        "Error proxying call to main node in method `{}`: {err}",
                        meta.name()
                    );
                });
                Web3Error::SubmitTransactionError(err.as_ref().to_string(), self.data())
            }
            _ => Web3Error::SubmitTransactionError(self.to_string(), self.data()),
        }
    }
}

// FIXME: remove in favor of wrapping in `InternalError`?
pub(crate) fn internal_error(error: impl fmt::Display) -> Web3Error {
    MethodMetadata::with(|meta| {
        meta.set_internal_error(&error.to_string());
    });
    Web3Error::InternalError
}
