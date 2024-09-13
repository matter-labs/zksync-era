use std::borrow::Cow;

use crate::CompressedBytecodeInfo;

/// Errors related to bytecode compression.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BytecodeCompressionError {
    #[error("Bytecode compression failed")]
    BytecodeCompressionFailed,
}

/// Result of compressing bytecodes used by a transaction.
pub type BytecodeCompressionResult<'a> =
    Result<Cow<'a, [CompressedBytecodeInfo]>, BytecodeCompressionError>;
