/// Backend "glue" which ties the actual Web3 API implementation to the `jsonrpsee` JSON RPC backend.
///
/// Consists mostly of boilerplate code implementing the `jsonrpsee` server traits for the corresponding
/// namespace structures defined in `zksync_core`.
use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned},
};

pub(crate) use self::{
    metadata::{MethodMetadata, MethodTracer},
    middleware::{
        CorrelationMiddleware, LimitMiddleware, MetadataLayer, ServerTimeoutMiddleware,
        ShutdownMiddleware, TrafficTracker,
    },
};
use crate::tx_sender::SubmitTxError;

mod metadata;
mod middleware;
pub mod namespaces;
#[cfg(test)]
pub(crate) mod testonly;

impl MethodTracer {
    pub(crate) fn map_err(&self, err: Web3Error) -> ErrorObjectOwned {
        self.observe_error(&err);

        let data = match &err {
            Web3Error::SubmitTransactionError(_, data) => Some(format!("0x{}", hex::encode(data))),
            Web3Error::ProxyError(_) => Some("0x".to_owned()),
            _ => None,
        };
        let code = match err {
            Web3Error::MethodNotImplemented => ErrorCode::MethodNotFound.code(),
            Web3Error::InternalError(_) => ErrorCode::InternalError.code(),
            Web3Error::NoBlock
            | Web3Error::PrunedBlock(_)
            | Web3Error::PrunedL1Batch(_)
            | Web3Error::TooManyTopics
            | Web3Error::FilterNotFound
            | Web3Error::InvalidFilterBlockHash
            | Web3Error::InvalidTimeout(_)
            | Web3Error::LogsLimitExceeded(_, _, _) => ErrorCode::InvalidParams.code(),
            Web3Error::SubmitTransactionError(_, _)
            | Web3Error::SerializationError(_)
            | Web3Error::ProxyError(_) => 3,
            Web3Error::TransactionTimeout(_) => 4,
            Web3Error::TransactionUnready(_) => 5,
            Web3Error::TreeApiUnavailable => 6,
            Web3Error::ServerShuttingDown => ErrorCode::ServerIsBusy.code(),
        };
        let message = match err {
            // Do not expose internal error details to the client.
            Web3Error::InternalError(_) => "Internal error".to_owned(),
            Web3Error::ProxyError(err) => err.as_ref().to_string(),
            Web3Error::SubmitTransactionError(message, _) => message,
            _ => err.to_string(),
        };

        ErrorObjectOwned::owned(code, message, data)
    }

    pub(crate) fn map_submit_err(&self, err: SubmitTxError) -> Web3Error {
        self.observe_submit_error(&err);

        match err {
            SubmitTxError::Internal(err) => Web3Error::InternalError(err),
            SubmitTxError::ProxyError(err) => Web3Error::ProxyError(err),
            SubmitTxError::ServerShuttingDown => Web3Error::ServerShuttingDown,
            _ => Web3Error::SubmitTransactionError(err.to_string(), err.data()),
        }
    }
}
