/// Errors related to bytecode compression.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BytecodeCompressionError {
    #[error("Bytecode compression failed")]
    BytecodeCompressionFailed,
}
