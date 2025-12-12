//! Definition of errors that can occur in the ZKsync Web3 API.

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

use jsonrpsee::{core::ClientError, types::error::ErrorCode};
use pin_project_lite::pin_project;
use thiserror::Error;
use zksync_types::{api::SerializationTransactionError, L1BatchNumber, L2BlockNumber};

/// Server-side representation of the RPC error.
#[derive(Debug, Error)]
pub enum Web3Error {
    #[error("Block with such an ID doesn't exist yet")]
    NoBlock,
    #[error("Block with such an ID is pruned; the first retained block is {0}")]
    PrunedBlock(L2BlockNumber),
    #[error("L1 batch with such an ID is pruned; the first retained L1 batch is {0}")]
    PrunedL1Batch(L1BatchNumber),
    #[error("{}", _0.as_ref())]
    ProxyError(#[from] EnrichedClientError),
    #[error("{0}")]
    SubmitTransactionError(String, Vec<u8>),
    #[error("Failed to serialize transaction: {0}")]
    SerializationError(#[from] SerializationTransactionError),
    #[error("More than four topics in filter")]
    TooManyTopics,
    #[error("Filter not found")]
    FilterNotFound,
    #[error("Query returned more than {0} results. Try with this block range [{1:#x}, {2:#x}].")]
    LogsLimitExceeded(usize, u32, u32),
    #[error("invalid filter: if blockHash is supplied fromBlock and toBlock must not be")]
    InvalidFilterBlockHash,
    /// Weaker form of a "method not found" error; the method implementation is technically present,
    /// but the node configuration prevents the method from functioning.
    #[error("Method not implemented")]
    MethodNotImplemented,
    /// Unavailability caused by node configuration is returned as [`Self::MethodNotImplemented`].
    #[error("Tree API is temporarily unavailable")]
    TreeApiUnavailable,
    #[error("Internal error")]
    InternalError(#[from] anyhow::Error),
    #[error("Server is shutting down")]
    ServerShuttingDown,
    #[error("Transaction {0:?} timeout waiting for receipt")]
    TransactionTimeout(zksync_types::H256),
    #[error("Transaction processing error: {0}")]
    TransactionUnready(String),
    #[error("Invalid timeout. Max timeout is {0}ms")]
    InvalidTimeout(u64),
}

/// Client RPC error with additional details: the method name and arguments of the called method.
///
/// The wrapped error can be accessed using [`AsRef`].
#[derive(Debug)]
pub struct EnrichedClientError {
    inner_error: ClientError,
    method: &'static str,
    args: HashMap<&'static str, String>,
}

/// Whether the error should be considered retriable.
pub fn is_retryable(err: &ClientError) -> bool {
    match err {
        ClientError::Transport(_) | ClientError::RequestTimeout => true,
        ClientError::Call(err) => {
            // At least some RPC providers use "internal error" in case of the server being overloaded

            err.code() == ErrorCode::ServerIsBusy.code()
                || err.code() == ErrorCode::InternalError.code()
        }
        _ => false,
    }
}

/// Alias for a result with enriched client RPC error.
pub type EnrichedClientResult<T> = Result<T, EnrichedClientError>;

impl EnrichedClientError {
    /// Wraps the specified `inner_error`.
    pub fn new(inner_error: ClientError, method: &'static str) -> Self {
        Self {
            inner_error,
            method,
            args: HashMap::new(),
        }
    }

    /// Creates an error wrapping [`RpcError::Custom`].
    pub fn custom(message: impl Into<String>, method: &'static str) -> Self {
        Self::new(ClientError::Custom(message.into()), method)
    }

    /// Adds a tracked argument for this error.
    #[must_use]
    pub fn with_arg(mut self, name: &'static str, value: &dyn fmt::Debug) -> Self {
        self.args.insert(name, format!("{value:?}"));
        self
    }

    /// Whether the error should be considered retryable.
    pub fn is_retryable(&self) -> bool {
        is_retryable(&self.inner_error)
    }

    pub fn is_timeout(&self) -> bool {
        matches!(self.inner_error, ClientError::RequestTimeout)
    }
}

impl AsRef<ClientError> for EnrichedClientError {
    fn as_ref(&self) -> &ClientError {
        &self.inner_error
    }
}

impl error::Error for EnrichedClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.inner_error)
    }
}

impl fmt::Display for EnrichedClientError {
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
    /// Contextual information about an RPC. Returned by [`ClientRpcContext::rpc_context()`]. The context is eventually converted
    /// to a result with [`EnrichedClientError`] error type.
    #[derive(Debug)]
    pub struct ClientCallWrapper<'a, F> {
        #[pin]
        inner: F,
        method: &'static str,
        args: HashMap<&'static str, &'a (dyn fmt::Debug + Send + Sync)>,
    }
}

impl<'a, T, F> ClientCallWrapper<'a, F>
where
    F: Future<Output = Result<T, ClientError>>,
{
    /// Adds a tracked argument for this context.
    #[must_use]
    pub fn with_arg(
        mut self,
        name: &'static str,
        value: &'a (dyn fmt::Debug + Send + Sync),
    ) -> Self {
        self.args.insert(name, value);
        self
    }
}

impl<T, F> Future for ClientCallWrapper<'_, F>
where
    F: Future<Output = Result<T, ClientError>>,
{
    type Output = Result<T, EnrichedClientError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let projection = self.project();
        match projection.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(value)) => Poll::Ready(Ok(value)),
            Poll::Ready(Err(err)) => {
                let err = EnrichedClientError {
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

/// Extension trait allowing to add context to client RPC calls. Can be used on any future resolving to `Result<_, ClientError>`.
pub trait ClientRpcContext: Sized {
    /// Adds basic context information: the name of the invoked RPC method.
    fn rpc_context(self, method: &'static str) -> ClientCallWrapper<'static, Self>;
}

impl<T, F> ClientRpcContext for F
where
    F: Future<Output = Result<T, ClientError>>,
{
    fn rpc_context(self, method: &'static str) -> ClientCallWrapper<'static, Self> {
        ClientCallWrapper {
            inner: self,
            method,
            args: HashMap::new(),
        }
    }
}
