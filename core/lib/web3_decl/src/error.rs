//! Definition of errors that can occur in the zkSync Web3 API.

use std::{collections::HashMap, error, error::Error, fmt, fmt::Debug};

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

#[derive(Debug)]
pub struct RpcErrorWithDetails {
    inner_error: RpcError,
    method: &'static str,
    args: HashMap<&'static str, String>,
}
impl error::Error for RpcErrorWithDetails {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.inner_error)
    }
}

impl fmt::Display for RpcErrorWithDetails {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "JsonRPC request: {} with args: {:?} failed because of: {}",
            self.method, self.args, self.inner_error
        )
    }
}

pub trait EnrichRpcError<T> {
    fn rpc_context(self, method: &'static str) -> Result<T, RpcErrorWithDetails>;
}

impl<T> EnrichRpcError<T> for Result<T, RpcError> {
    fn rpc_context(self, method: &'static str) -> Result<T, RpcErrorWithDetails> {
        match self {
            Ok(t) => Ok(t),
            Err(error) => Err(RpcErrorWithDetails {
                inner_error: error,
                method: method,
                args: HashMap::default(),
            }),
        }
    }
}

impl RpcErrorWithDetails {
    pub fn inner(&self) -> &RpcError {
        &self.inner_error
    }
}

pub trait WithArgRpcError<T> {
    fn with_arg(self, arg: &'static str, value: &dyn fmt::Debug) -> Result<T, RpcErrorWithDetails>;
}

impl<T> WithArgRpcError<T> for Result<T, RpcErrorWithDetails> {
    fn with_arg(self, arg: &'static str, value: &dyn Debug) -> Result<T, RpcErrorWithDetails> {
        match self {
            Ok(t) => Ok(t),
            Err(error) => {
                let mut new_args = error.args;
                new_args.insert(arg, format!("{value:?}"));
                Err(RpcErrorWithDetails {
                    inner_error: error.inner_error,
                    method: error.method,
                    args: new_args,
                })
            }
        }
    }
}
