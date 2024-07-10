use thiserror::Error;

#[derive(Debug, Error)]
pub enum XL2TxParseError {
    #[error("PubData length mismatch")]
    PubdataLengthMismatch,
    #[error("Unsupported priority op type")]
    UnsupportedPriorityOpType,
    #[error("Unexpected priority queue type")]
    UnexpectedPriorityQueueType,
    #[error("Ethereum ABI error: {0}")]
    AbiError(#[from] crate::ethabi::Error),
}
