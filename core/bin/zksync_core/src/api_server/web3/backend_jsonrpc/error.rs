use jsonrpc_core::{Error, ErrorCode};
use zksync_web3_decl::error::Web3Error;

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
            | Web3Error::InvalidFilterBlockHash => ErrorCode::InvalidParams,
            Web3Error::SubmitTransactionError(_, _) | Web3Error::SerializationError(_) => 3.into(),
            Web3Error::PubSubTimeout => 4.into(),
            Web3Error::RequestTimeout => 5.into(),
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

pub fn internal_error(method_name: &str, error: impl ToString) -> Web3Error {
    vlog::error!(
        "Internal error in method {}: {}",
        method_name,
        error.to_string(),
    );
    metrics::counter!("api.web3.internal_errors", 1, "method" => method_name.to_string());

    Web3Error::InternalError
}
