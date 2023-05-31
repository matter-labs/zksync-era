use zksync_eth_client::types::Error;

#[derive(Debug, thiserror::Error)]
pub enum ETHSenderError {
    #[error("Ethereum gateway Error {0}")]
    EthereumGateWayError(#[from] Error),
}
