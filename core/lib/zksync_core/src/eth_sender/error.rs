use zksync_eth_client::types;
use zksync_types::web3::contract;

#[derive(Debug, thiserror::Error)]
pub enum ETHSenderError {
    #[error("Ethereum gateway Error {0}")]
    EthereumGateWayError(#[from] types::Error),
    #[error("Token parsing Error: {0}")]
    ParseError(#[from] contract::Error),
}
