use crate::CompressedBytecodeInfo;

/// Errors related to bytecode compression.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BytecodeCompressionError {
    #[error("Bytecode compression failed")]
    BytecodeCompressionFailed,
}

/// Result of compressing bytecodes used by a transaction.
pub type BytecodeCompressionResult = Result<Vec<CompressedBytecodeInfo>, BytecodeCompressionError>;
