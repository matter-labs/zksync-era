//! Backend "glue" which ties the actual Web3 API implementation to the `jsonrpsee` JSON RPC backend.
//! Consists mostly of boilerplate code implementing the `jsonrpsee` server traits for the corresponding
//! namespace structures defined in `zksync_core`.

use std::{borrow::Cow, fmt, mem};

use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::types::{error::ErrorCode, ErrorObjectOwned},
};

pub(crate) use self::{
    metadata::{MethodMetadata, MethodTracer},
    middleware::{LimitMiddleware, MetadataMiddleware, ShutdownMiddleware, TrafficTracker},
};
use crate::api_server::tx_sender::SubmitTxError;

mod metadata;
mod middleware;
pub mod namespaces;
#[cfg(test)]
pub(crate) mod testonly;

type RawParams<'a> = Option<Cow<'a, serde_json::value::RawValue>>;

/// Version of `RawParams` (i.e., params of a JSON-RPC request) that is statically known to provide a borrow
/// to another `RawParams` with the same lifetime.
///
/// # Why?
///
/// We need all this complexity because we'd like to access request params in a generic way without an overhead
/// (i.e., cloning strings for each request – which would hit performance and fragment the heap).
/// One could think that `jsonrpsee` should borrow params by default;
/// if that were the case, we could opportunistically expect `RawParams` to be `None` or `Some(Cow::Borrowed(_))`
/// and just copy `&RawValue` in the latter case. In practice, `RawParams` are *never* `Some(Cow::Borrowed(_))`
/// because of a bug (?) with deserializing `Cow<'_, RawValue>`: https://github.com/serde-rs/json/issues/1076
struct RawParamsWithBorrow<'a>(RawParams<'a>);

impl fmt::Debug for RawParamsWithBorrow<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RawParamsWithBorrow")
            .field(&self.get())
            .finish()
    }
}

impl<'a> RawParamsWithBorrow<'a> {
    /// SAFETY: the returned params must outlive `raw_params`.
    unsafe fn new(raw_params: &mut RawParams<'a>) -> Self {
        Self(match &*raw_params {
            None => None,
            Some(Cow::Borrowed(raw_value)) => Some(Cow::Borrowed(*raw_value)),
            Some(Cow::Owned(raw_value)) => {
                let raw_value_ref: &serde_json::value::RawValue = raw_value;
                // SAFETY: We extend the lifetime to 'a. This is only safe under the following conditions:
                //
                // - The reference points to a stable memory location (it is; `raw_value` is `&Box<RawValue>`, i.e., heap-allocated)
                // - `raw_value` outlives the reference (guaranteed by the method contract)
                // - `raw_value` is never mutated or provides a mutable reference (guaranteed by the `BorrowingRawParams` API)
                let raw_value_ref: &'a serde_json::value::RawValue = mem::transmute(raw_value_ref);
                mem::replace(raw_params, Some(Cow::Borrowed(raw_value_ref)))
            }
        })
    }

    fn get(&self) -> Option<&str> {
        self.0.as_deref().map(serde_json::value::RawValue::get)
    }
}

impl MethodTracer {
    pub(crate) fn map_err(&self, err: Web3Error) -> ErrorObjectOwned {
        self.observe_error(&err);

        let data = match &err {
            Web3Error::SubmitTransactionError(_, data) => Some(format!("0x{}", hex::encode(data))),
            Web3Error::ProxyError(_) => Some("0x".to_owned()),
            _ => None,
        };
        let code = match err {
            Web3Error::NotImplemented => ErrorCode::MethodNotFound.code(),
            Web3Error::InternalError(_) => ErrorCode::InternalError.code(),
            Web3Error::NoBlock
            | Web3Error::PrunedBlock(_)
            | Web3Error::PrunedL1Batch(_)
            | Web3Error::TooManyTopics
            | Web3Error::FilterNotFound
            | Web3Error::InvalidFilterBlockHash
            | Web3Error::LogsLimitExceeded(_, _, _) => ErrorCode::InvalidParams.code(),
            Web3Error::SubmitTransactionError(_, _)
            | Web3Error::SerializationError(_)
            | Web3Error::ProxyError(_) => 3,
            Web3Error::TreeApiUnavailable => 6,
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
}

impl From<SubmitTxError> for Web3Error {
    fn from(err: SubmitTxError) -> Self {
        match err {
            SubmitTxError::Internal(err) => Self::InternalError(err),
            SubmitTxError::ProxyError(err) => Self::ProxyError(err),
            _ => Self::SubmitTransactionError(err.to_string(), err.data()),
        }
    }
}
