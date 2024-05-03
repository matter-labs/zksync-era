use thiserror::Error;

/// Errors related to bytecode compression.
#[derive(Debug, Error)]
pub enum BytecodeCompressionError {
    #[error("Bytecode compression failed")]
    BytecodeCompressionFailed,
}
