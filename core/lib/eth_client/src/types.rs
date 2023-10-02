// External uses
use zksync_types::web3::{
    ethabi,
    types::{TransactionReceipt, H256, U256},
};

/// Common error type exposed by the crate,
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Problem on the Ethereum client side (e.g. bad RPC call, network issues).
    #[error("Request to ethereum gateway failed: {0}")]
    EthereumGateway(#[from] zksync_types::web3::Error),
    /// Problem with a contract call.
    #[error("Call to contract failed: {0}")]
    Contract(#[from] zksync_types::web3::contract::Error),
    /// Problem with transaction signer.
    #[error("Transaction signing failed: {0}")]
    Signer(#[from] zksync_eth_signer::error::SignerError),
    /// Problem with transaction decoding.
    #[error("Decoding revert reason failed: {0}")]
    Decode(#[from] ethabi::Error),
    /// Incorrect fee provided for a transaction.
    #[error("Max fee {0} less than priority fee {1}")]
    WrongFeeProvided(U256, U256),
}

/// Representation of a signed transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedCallResult {
    /// Raw transaction bytes.
    pub raw_tx: Vec<u8>,
    /// `max_priority_fee_per_gas` field of transaction (EIP1559).
    pub max_priority_fee_per_gas: U256,
    /// `max_fee_per_gas` field of transaction (EIP1559).
    pub max_fee_per_gas: U256,
    /// `nonce` field of transaction.
    pub nonce: U256,
    /// Transaction hash.
    pub hash: H256,
}

/// State of the executed Ethereum transaction.
#[derive(Debug, Clone)]
pub struct ExecutedTxStatus {
    /// The hash of the executed L1 transaction.
    pub tx_hash: H256,
    /// Whether transaction was executed successfully or failed.
    pub success: bool,
    /// Receipt for a transaction.
    pub receipt: TransactionReceipt,
}

/// Information about transaction failure.
///
/// Two common reasons for transaction failure are:
/// - Revert
/// - Out of gas.
///
/// This structure tries to provide information about both of them.
#[derive(Debug, Clone)]
pub struct FailureInfo {
    /// RPC error code.
    pub revert_code: i64,
    /// RPC error message (normally, for a reverted transaction it would
    /// include the revert reason).
    pub revert_reason: String,
    /// Amount of gas used by transaction (may be `None` for `eth_call` requests).
    pub gas_used: Option<U256>,
    /// Gas limit of the transaction.
    pub gas_limit: U256,
}
