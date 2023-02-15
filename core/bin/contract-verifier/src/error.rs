#[derive(Debug, Clone, thiserror::Error)]
pub enum ContractVerifierError {
    #[error("Internal error")]
    InternalError,
    #[error("Deployed bytecode is not equal to generated one from given source")]
    BytecodeMismatch,
    #[error("Constructor arguments are not correct")]
    IncorrectConstructorArguments,
    #[error("Compilation takes too much time")]
    CompilationTimeout,
    #[error("ZkSolc error: {0}")]
    ZkSolcError(String),
    #[error("Compilation error")]
    CompilationError(serde_json::Value),
    #[error("Unknown zksolc version: {0}")]
    UnknownZkSolcVersion(String),
    #[error("Unknown solc version: {0}")]
    UnknownSolcVersion(String),
    #[error("Contract with {0} name is missing in sources")]
    MissingContract(String),
    #[error("There is no {0} source file")]
    MissingSource(String),
    #[error("Contract with {0} name is an abstract and thus is not verifiable")]
    AbstractContract(String),
    #[error("Failed to deserialize standard JSON input")]
    FailedToDeserializeInput,
}
