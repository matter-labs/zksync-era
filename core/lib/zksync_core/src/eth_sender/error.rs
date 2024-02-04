use zksync_types::web3::contract;

#[derive(Debug, thiserror::Error)]
pub enum ETHSenderError {
    #[error("Ethereum gateway Error {0}")]
    EthereumGateWayError(#[from] zksync_eth_client::Error),
    #[error("Token parsing Error: {0}")]
    ParseError(#[from] contract::Error),
}
