use zksync_dal::DalError;

#[derive(Debug, thiserror::Error)]
pub enum ContractVerifierError {
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
    #[error("Deployed bytecode is not equal to generated one from given source")]
    BytecodeMismatch,
    #[error("Creation bytecode is not equal to generated one from given source")]
    CreationBytecodeMismatch,
    #[error("Constructor arguments are not correct")]
    IncorrectConstructorArguments,
    #[error("Compilation takes too much time")]
    CompilationTimeout,
    #[error("{0} error: {1}")]
    CompilerError(&'static str, String),
    #[error("Compilation error")]
    CompilationError(serde_json::Value),
    #[error("Unknown {0} version: {1}")]
    UnknownCompilerVersion(&'static str, String),
    #[error("Contract with {0} name is missing in sources")]
    MissingContract(String),
    #[error("There is no {0} source file")]
    MissingSource(String),
    #[error("Contract with {0} name is an abstract and thus is not verifiable")]
    AbstractContract(String),
    #[error("Compiler output for contract {contract_name} is missing required field {field_path}")]
    MissingCompilerOutput {
        contract_name: String,
        field_path: &'static str,
    },
    #[error("Failed to deserialize standard JSON input")]
    FailedToDeserializeInput,
    #[error("Source path is not allowed: {0}")]
    InvalidSourcePath(String),
}

impl From<DalError> for ContractVerifierError {
    fn from(err: DalError) -> Self {
        Self::Internal(err.generalize())
    }
}
