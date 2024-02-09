//! Definition of errors that can occur in the zkSync Web3 API.

use std::{
    collections::HashMap,
    error,
    error::Error,
    fmt,
    future::Future,
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use jsonrpsee::core::ClientError as RpcError;
use pin_project_lite::pin_project;
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

impl RpcErrorWithDetails {
    pub fn new(inner_error: RpcError, method: &'static str) -> Self {
        Self {
            inner_error,
            method,
            args: HashMap::new(),
        }
    }

    pub fn with_arg(mut self, name: &'static str, value: &dyn fmt::Debug) -> Self {
        self.args.insert(name, format!("{value:?}"));
        self
    }
}

impl AsRef<RpcError> for RpcErrorWithDetails {
    fn as_ref(&self) -> &RpcError {
        &self.inner_error
    }
}

impl error::Error for RpcErrorWithDetails {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.inner_error)
    }
}

impl fmt::Display for RpcErrorWithDetails {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct DebugArgs<'a>(&'a HashMap<&'static str, String>);

        impl fmt::Debug for DebugArgs<'_> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("(")?;
                for (i, (name, value)) in self.0.iter().enumerate() {
                    write!(formatter, "{name}={value}")?;
                    if i + 1 < self.0.len() {
                        formatter.write_str(", ")?;
                    }
                }
                formatter.write_str(")")
            }
        }

        write!(
            formatter,
            "JSON-RPC request {}{:?} failed: {}",
            self.method,
            DebugArgs(&self.args),
            self.inner_error
        )
    }
}

pin_project! {
    #[derive(Debug)]
    pub struct RpcContext<'a, F> {
        #[pin]
        inner: F,
        method: &'static str,
        args: HashMap<&'static str, &'a (dyn fmt::Debug + Send + Sync)>,
    }
}

impl<'a, T, F> RpcContext<'a, F>
where
    F: Future<Output = Result<T, RpcError>>,
{
    pub fn with_arg(
        mut self,
        name: &'static str,
        value: &'a (dyn fmt::Debug + Send + Sync),
    ) -> Self {
        self.args.insert(name, value);
        self
    }
}

impl<T, F> Future for RpcContext<'_, F>
where
    F: Future<Output = Result<T, RpcError>>,
{
    type Output = Result<T, RpcErrorWithDetails>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let projection = self.project();
        match projection.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(value)) => Poll::Ready(Ok(value)),
            Poll::Ready(Err(err)) => {
                let err = RpcErrorWithDetails {
                    inner_error: err,
                    method: projection.method,
                    // `mem::take()` is safe to use: by contract, a `Future` shouldn't be polled after completion
                    args: mem::take(projection.args)
                        .into_iter()
                        .map(|(name, value)| (name, format!("{value:?}")))
                        .collect(),
                };
                Poll::Ready(Err(err))
            }
        }
    }
}

pub trait EnrichRpcError: Sized {
    fn rpc_context(self, method: &'static str) -> RpcContext<Self>;
}

impl<T, F> EnrichRpcError for F
where
    F: Future<Output = Result<T, RpcError>>,
{
    fn rpc_context(self, method: &'static str) -> RpcContext<Self> {
        RpcContext {
            inner: self,
            method,
            args: HashMap::new(),
        }
    }
}
