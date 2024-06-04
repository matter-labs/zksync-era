use zksync_eth_signer::error::SignerError;
pub use zksync_web3_decl::jsonrpsee::core::ClientError as RpcError;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Network '{0}' is not supported")]
    NetworkNotSupported(String),
    #[error("Unable to decode server response: {0}")]
    MalformedResponse(String),
    #[error("RPC error: {0:?}")]
    RpcError(#[from] RpcError),
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Provided account credentials are incorrect")]
    IncorrectCredentials,
    #[error("Incorrect address")]
    IncorrectAddress,

    #[error("Operation timeout")]
    OperationTimeout,
    #[error("Polling interval is too small")]
    PollingIntervalIsTooSmall,

    #[error("Signing error: {0}")]
    SigningError(#[from] SignerError),
    #[error("Missing required field for a transaction: {0}")]
    MissingRequiredField(String),

    #[error("Failed to estimate the cost of the contract execution")]
    ExecutionContractEstimationError,

    #[error("Ethereum private key was not provided for this wallet")]
    NoEthereumPrivateKey,

    #[error("Provided value is not packable")]
    NotPackableValue,

    #[error("Provided function arguments are incorrect")]
    IncorrectInput,

    #[error("Other")]
    Other,

    #[error("EVM Deploy Error: {0}")]
    EvmDeploy(String),
}
