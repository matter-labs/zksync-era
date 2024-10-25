use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tonic::transport::{Channel, ClientTlsConfig};
use zksync_config::configs::da_client::eigen_da::{DisperserConfig, EigenDAConfig};
use zksync_da_client::{
    types::{self},
    DataAvailabilityClient,
};

use super::{
    disperser::{self, disperser_client::DisperserClient, BlobStatusRequest, DisperseBlobRequest},
    memstore::MemStore,
};

#[derive(Clone, Debug)]
enum Disperser {
    Remote(RemoteClient),
    Memory(Arc<MemStore>),
}

#[derive(Clone, Debug)]
struct RemoteClient {
    pub disperser: Arc<Mutex<DisperserClient<Channel>>>,
    pub config: DisperserConfig,
}

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
            Disperser::Remote(remote_disperser) => {
                let config = remote_disperser.config.clone();
                let custom_quorum_numbers = config.custom_quorum_numbers.unwrap_or_default();
                let account_id = config.account_id.unwrap_or_default();
                let request_id = remote_disperser
                    .disperser
                    .lock()
                    .await
                    .disperse_blob(DisperseBlobRequest {
                        data: blob_data,
                        custom_quorum_numbers,
                        account_id,
                    })
                    .await
                    .unwrap()
                    .into_inner()
                    .request_id;
                Ok(types::DispatchResponse {
                    blob_id: hex::encode(request_id),
                })
            }
            Disperser::Memory(memstore) => {
                let request_id = memstore
                    .clone()
                    .put_blob(blob_data)
                    .await
                    .map_err(|err| to_retriable_error(err.into()))?;
                Ok(types::DispatchResponse {
                    blob_id: hex::encode(request_id),
                })
            }
        }
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<types::InclusionData>, types::DAError> {
        match &self.disperser {
            Disperser::Remote(remote_client) => {
                let request_id = hex::decode(blob_id).unwrap();
                let blob_status_reply = remote_client
                    .disperser
                    .lock()
                    .await
                    .get_blob_status(BlobStatusRequest { request_id })
                    .await
                    .unwrap()
                    .into_inner();
                let blob_status = blob_status_reply.status();
                match blob_status {
                    disperser::BlobStatus::Unknown => Err(to_retriable_error(anyhow::anyhow!(
                        "Blob status is unknown"
                    ))),
                    disperser::BlobStatus::Processing => Err(to_retriable_error(anyhow::anyhow!(
                        "Blob is being processed"
                    ))),
                    disperser::BlobStatus::Confirmed => {
                        if remote_client.config.wait_for_finalization {
                            Err(to_retriable_error(anyhow::anyhow!(
                                "Blob is confirmed but not finalized"
                            )))
                        } else {
                            Ok(Some(types::InclusionData {
                                data: blob_status_reply
                                    .info
                                    .unwrap()
                                    .blob_verification_proof
                                    .unwrap()
                                    .inclusion_proof,
                            }))
                        }
                    }
                    disperser::BlobStatus::Failed => {
                        Err(to_non_retriable_error(anyhow::anyhow!("Blob has failed")))
                    }
                    disperser::BlobStatus::InsufficientSignatures => Err(to_non_retriable_error(
                        anyhow::anyhow!("Insufficient signatures for blob"),
                    )),
                    disperser::BlobStatus::Dispersing => Err(to_retriable_error(anyhow::anyhow!(
                        "Blob is being dispersed"
                    ))),
                    disperser::BlobStatus::Finalized => Ok(Some(types::InclusionData {
                        data: blob_status_reply
                            .info
                            .unwrap()
                            .blob_verification_proof
                            .unwrap()
                            .inclusion_proof,
                    })),
                }
            }
            Disperser::Memory(memstore) => {
                let request_id = hex::decode(blob_id).unwrap();
                let data = memstore
                    .clone()
                    .get_blob(request_id)
                    .await
                    .map_err(|err| to_retriable_error(err.into()))?;
                Ok(Some(types::InclusionData { data }))
            }
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(Self::BLOB_SIZE_LIMIT_IN_BYTES)
    }
}

fn to_retriable_error(error: anyhow::Error) -> types::DAError {
    types::DAError {
        error,
        is_retriable: true,
    }
}

fn to_non_retriable_error(error: anyhow::Error) -> types::DAError {
    types::DAError {
        error,
        is_retriable: false,
    }
}

#[cfg(test)]
mod test {
    use zksync_config::configs::da_client::eigen_da::MemStoreConfig;

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
