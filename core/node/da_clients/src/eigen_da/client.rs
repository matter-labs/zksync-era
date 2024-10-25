use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tonic::transport::{Channel, ClientTlsConfig};
use zksync_config::configs::da_client::eigen_da::EigenDAConfig;
use zksync_da_client::{
    types::{self},
    DataAvailabilityClient,
};

use super::disperser_clients::{
    disperser::disperser_client::DisperserClient, memstore::MemStore, remote::RemoteClient,
    Disperser,
};

#[derive(Clone, Debug)]
pub struct EigenDAClient {
    disperser: Disperser,
}

impl EigenDAClient {
    pub const BLOB_SIZE_LIMIT_IN_BYTES: usize = 2 * 1024 * 1024; // 2MB

    pub async fn new(config: EigenDAConfig) -> anyhow::Result<Self> {
        let disperser: Disperser = match config.clone() {
            EigenDAConfig::Disperser(config) => {
                match rustls::crypto::ring::default_provider().install_default() {
                    Ok(_) => {}
                    Err(_) => {} // This is not an actual error, we expect this function to return an Err(Arc<CryptoProvider>)
                };

                let inner = Channel::builder(config.disperser_rpc.parse()?)
                    .tls_config(ClientTlsConfig::new().with_native_roots())?;
                let disperser = Arc::new(Mutex::new(DisperserClient::connect(inner).await?));
                Disperser::Remote(RemoteClient { disperser, config })
            }
            EigenDAConfig::MemStore(config) => Disperser::Memory(MemStore::new(config)),
        };
        Ok(Self { disperser })
    }
}

#[async_trait]
impl DataAvailabilityClient for EigenDAClient {
    async fn dispatch_blob(
        &self,
        _batch_number: u32,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        match &self.disperser {
            Disperser::Remote(remote_disperser) => remote_disperser.disperse_blob(blob_data).await,
            Disperser::Memory(memstore) => memstore.clone().store_blob(blob_data).await,
        }
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<types::InclusionData>, types::DAError> {
        match &self.disperser {
            Disperser::Remote(remote_client) => remote_client.get_inclusion_data(blob_id).await,
            Disperser::Memory(memstore) => memstore.clone().get_inclusion_data(blob_id).await,
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(Self::BLOB_SIZE_LIMIT_IN_BYTES)
    }
}

pub fn to_retriable_error(error: anyhow::Error) -> types::DAError {
    types::DAError {
        error,
        is_retriable: true,
    }
}

pub fn to_non_retriable_error(error: anyhow::Error) -> types::DAError {
    types::DAError {
        error,
        is_retriable: false,
    }
}

#[cfg(test)]
mod test {
    use zksync_config::configs::da_client::eigen_da::{DisperserConfig, MemStoreConfig};

    use super::*;

    #[tokio::test]
    async fn test_eigenda_memory_disperser() {
        let config = EigenDAConfig::MemStore(MemStoreConfig {
            api_node_url: "".to_string(),
            custom_quorum_numbers: Some(vec![]),
            account_id: Some("".to_string()),
            max_blob_size_bytes: 2 * 1024 * 1024, // 2MB,
            blob_expiration: 60 * 2,
            get_latency: 0,
            put_latency: 0,
        });
        let client = EigenDAClient::new(config).await.unwrap();
        let data = vec![1u8; 100];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let retrieved_data = client.get_inclusion_data(&result.blob_id).await.unwrap();
        assert_eq!(retrieved_data.unwrap().data, data);
    }

    #[tokio::test]
    async fn test_eigenda_remote_disperser() {
        let config = EigenDAConfig::Disperser(DisperserConfig {
            api_node_url: String::default(),
            custom_quorum_numbers: None,
            account_id: None,
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            eth_confirmation_depth: -1,
            eigenda_eth_rpc: String::default(),
            eigenda_svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            blob_size_limit: 2 * 1024 * 1024, // 2MB
            status_query_timeout: 1800,       // 30 minutes
            status_query_interval: 5,         // 5 seconds
            wait_for_finalization: false,
        });
        let client = EigenDAClient::new(config).await.unwrap();
        let data = vec![1u8; 100];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        loop {
            let query_result = client.get_inclusion_data(&result.blob_id).await;
            match query_result {
                Err(err) => {
                    if err.is_retriable {
                        println!("Retriable error: {:?}", err.error);
                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                    } else {
                        panic!("Error: {:?}", err.error);
                    }
                }
                Ok(inclusion_data) => {
                    assert!(inclusion_data.is_some());
                    break;
                }
            }
        }
    }
}
