use jsonrpc_core::{Error, ErrorCode};
use zksync_web3_decl::error::Web3Error;

use std::fmt;

use crate::api_server::web3::metrics::API_METRICS;

pub fn into_jsrpc_error(err: Web3Error) -> Error {
    Error {
        code: match err {
            Web3Error::InternalError | Web3Error::NotImplemented => ErrorCode::InternalError,
            Web3Error::NoBlock
            | Web3Error::NoSuchFunction
            | Web3Error::RLPError(_)
            | Web3Error::InvalidTransactionData(_)
            | Web3Error::TooManyTopics
            | Web3Error::FilterNotFound
            | Web3Error::InvalidFeeParams(_)
            | Web3Error::LogsLimitExceeded(_, _, _)
            | Web3Error::TooManyLogs(_)
            | Web3Error::InvalidFilterBlockHash => ErrorCode::InvalidParams,
            Web3Error::SubmitTransactionError(_, _) | Web3Error::SerializationError(_) => 3.into(),
            Web3Error::PubSubTimeout => 4.into(),
            Web3Error::RequestTimeout => 5.into(),
            Web3Error::TreeApiUnavailable => 6.into(),
        },
        message: match err {
            Web3Error::SubmitTransactionError(_, _) => err.to_string(),
            _ => err.to_string(),
        },
        data: match err {
            Web3Error::SubmitTransactionError(_, data) => {
                Some(format!("0x{}", hex::encode(data)).into())
            }
            _ => None,
        },
    }
}

pub fn internal_error(method_name: &'static str, error: impl fmt::Display) -> Web3Error {
    tracing::error!("Internal error in method {method_name}: {error}");
    API_METRICS.web3_internal_errors[&method_name].inc();
    Web3Error::InternalError
}
