use thiserror::Error;

#[derive(Error, Debug)]
pub enum CLIErrors {
    #[error(transparent)]
    ZkSyncEthClientError(#[from] zksync_eth_client::Error),
    #[error("{0}: {1}")]
    FromEnvError(String, String),
    #[error("{0}: {1}")]
    PostgresConfigError(String, String),
    #[error(transparent)]
    DalError(#[from] zksync_dal::DalError),
    #[error("{0}: {1}")]
    ConnectionPoolBuilderError(String, String),
    #[error(transparent)]
    ZkSyncBasicTypesError(#[from] zksync_basic_types::web3::contract::Error),
    #[error(transparent)]
    SqlxError(#[from] sqlx::Error),
    #[error(transparent)]
    AnyHowError(#[from] anyhow::Error),
}
