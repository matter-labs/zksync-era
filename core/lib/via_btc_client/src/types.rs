use bitcoin::Network;
use thiserror::Error;

pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Regtest,
}

impl From<BitcoinNetwork> for Network {
    fn from(network: BitcoinNetwork) -> Self {
        match network {
            BitcoinNetwork::Mainnet => Network::Bitcoin,
            BitcoinNetwork::Testnet => Network::Testnet,
            BitcoinNetwork::Regtest => Network::Regtest,
        }
    }
}

#[allow(unused)]
#[derive(Debug, Error)]
pub enum BitcoinError {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("Signing error: {0}")]
    SigningError(String),

    #[error("Inscription error: {0}")]
    InscriptionError(String),

    #[error("Indexing error: {0}")]
    IndexingError(String),

    #[error("Transaction building error: {0}")]
    TransactionBuildingError(String),

    #[error("Fee estimation error: {0}")]
    FeeEstimationFailed(String),

    #[error("Invalid network: {0}")]
    InvalidNetwork(String),

    #[error("Invalid output point: {0}")]
    InvalidOutpoint(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BitcoinError>;

pub type BitcoinClientResult<T> = Result<T>;
pub type BitcoinRpcResult<T> = Result<T>;

impl From<bitcoincore_rpc::Error> for BitcoinError {
    fn from(error: bitcoincore_rpc::Error) -> Self {
        BitcoinError::Rpc(error.to_string())
    }
}

impl From<bitcoin::address::ParseError> for BitcoinError {
    fn from(error: bitcoin::address::ParseError) -> Self {
        BitcoinError::InvalidAddress(error.to_string())
    }
}

impl From<bitcoin::hex::HexToArrayError> for BitcoinError {
    fn from(error: bitcoin::hex::HexToArrayError) -> Self {
        BitcoinError::InvalidTransaction(error.to_string())
    }
}

pub type BitcoinSignerResult<T> = Result<T>;
pub type BitcoinInscriberResult<T> = Result<T>;
pub type BitcoinInscriptionIndexerResult<T> = Result<T>;
pub type BitcoinTransactionBuilderResult<T> = Result<T>;
