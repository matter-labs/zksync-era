// Integration test for zkSync Rust SDK.
//
// In order to pass these tests, there must be a running
// instance of zkSync server and prover:
//
// ```bash
// zk server &!
// zk dummy-prover run &!
// zk test integration rust-sdk
// ```
//
// Note: If tests are failing, first check the following two things:
//
// 1. If tests are failing with an error "cannot operate after unexpected tx failure",
//    ensure that dummy prover is enabled.
// 2. If tests are failing with an error "replacement transaction underpriced",
//    ensure that tests are ran in one thread. Running the tests with many threads won't
//    work, since many thread will attempt in sending transactions from one (main) Ethereum
//    account, which may result in nonce mismatch.
//    Also, if there will be many tests running at once, and the server will die, it will be
//    hard to distinguish which test exactly caused this problem.

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
}
