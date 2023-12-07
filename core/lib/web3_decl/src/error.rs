//! Definition of errors that can occur in the zkSync Web3 API.

use thiserror::Error;
use zksync_types::api::SerializationTransactionError;

#[derive(Debug, Error)]
pub enum Web3Error {
    #[error("Block with such an ID doesn't exist yet")]
    NoBlock,
    #[error("Request timeout")]
    RequestTimeout,
    #[error("Internal error")]
    InternalError,
    #[error("RLP decoding error: {0}")]
    RLPError(#[from] rlp::DecoderError),
    #[error("No function with given signature found")]
    NoSuchFunction,
    #[error("Invalid transaction data: {0}")]
    InvalidTransactionData(#[from] zksync_types::ethabi::Error),
    #[error("{0}")]
    SubmitTransactionError(String, Vec<u8>),
    #[error("Failed to serialize transaction: {0}")]
    SerializationError(#[from] SerializationTransactionError),
    #[error("Invalid fee parameters: {0}")]
    InvalidFeeParams(String),
    #[error("More than four topics in filter")]
    TooManyTopics,
    #[error("Your connection time exceeded the limit")]
    PubSubTimeout,
    #[error("Filter not found")]
    FilterNotFound,
    #[error("Not implemented")]
    NotImplemented,
    #[error("Query returned more than {0} results. Try with this block range [{1:#x}, {2:#x}].")]
    LogsLimitExceeded(usize, u32, u32),
    #[error("invalid filter: if blockHash is supplied fromBlock and toBlock must not be")]
    InvalidFilterBlockHash,
    #[error("Query returned more than {0} results. Try smaller range of blocks")]
    TooManyLogs(usize),
    #[error("Tree API is not available")]
    TreeApiUnavailable,
}
