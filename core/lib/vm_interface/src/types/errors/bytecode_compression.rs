/// Errors related to bytecode compression.
#[derive(Debug, thiserror::Error)]
pub enum BytecodeCompressionError {
    #[error("Bytecode compression failed")]
    BytecodeCompressionFailed,
}
