//! Definition of errors that can occur in the zkSync Web3 API.

use jsonrpsee::core::ClientError as RpcError;
use thiserror::Error;
use zksync_types::{api::SerializationTransactionError, L1BatchNumber, MiniblockNumber};

#[derive(Debug, Error)]
pub enum Web3Error {
    #[error("Block with such an ID doesn't exist yet")]
    NoBlock,
    #[error("Block with such an ID is pruned; the first retained block is {0}")]
    PrunedBlock(MiniblockNumber),
    #[error("L1 batch with such an ID is pruned; the first retained L1 batch is {0}")]
    PrunedL1Batch(L1BatchNumber),
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
    #[error("Tree API is not available")]
    TreeApiUnavailable,
}

use std::{collections::HashMap, error, fmt, fmt::Debug};

#[derive(Debug)]
pub struct RpcErrorWithDetails {
    pub inner_error: RpcError,
    pub method: String,
    pub args: HashMap<String, String>,
}
impl error::Error for RpcErrorWithDetails {}

impl fmt::Display for RpcErrorWithDetails {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args_formatted: Vec<String> =
            self.args.iter().map(|(k, v)| format!("{k}: {v}")).collect();
        write!(
            formatter,
            "JsonRPC request: {} with args: {} failed because of: {}",
            self.method,
            args_formatted.join(", "),
            self.inner_error
        )
    }
}

pub trait EnrichRpcError<T> {
    fn rpc_context(self, method: &str) -> Result<T, RpcErrorWithDetails>;
}

impl<T> EnrichRpcError<T> for Result<T, RpcError> {
    fn rpc_context(self, method: &str) -> Result<T, RpcErrorWithDetails> {
        match self {
            Ok(t) => Ok(t),
            Err(error) => Err(RpcErrorWithDetails {
                inner_error: error,
                method: method.to_string(),
                args: HashMap::default(),
            }),
        }
    }
}

pub trait WithArgRpcError<T> {
    fn with_arg(self, arg: &str, value: &dyn fmt::Debug) -> Result<T, RpcErrorWithDetails>;
}

impl<T> WithArgRpcError<T> for Result<T, RpcErrorWithDetails> {
    fn with_arg(self, arg: &str, value: &dyn Debug) -> Result<T, RpcErrorWithDetails> {
        match self {
            Ok(t) => Ok(t),
            Err(error) => {
                let mut new_args = error.args.clone();
                new_args.insert(arg.to_string(), format!("{value:?}"));
                Err(RpcErrorWithDetails {
                    inner_error: error.inner_error,
                    method: error.method,
                    args: new_args,
                })
            }
        }
    }
}
