use std::{
    fmt::Debug,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{sync::Mutex, time::interval};
use tonic::transport::{Channel, ClientTlsConfig};
use zksync_config::configs::da_client::eigen_da::DisperserConfig;

use crate::{
    blob_info::BlobInfo,
    disperser::{self, disperser_client::DisperserClient, BlobStatusRequest, DisperseBlobRequest},
};

pub struct EigenDAProxyClient {
    disperser: Arc<Mutex<DisperserClient<Channel>>>,
    config: DisperserConfig,
}

impl EigenDAProxyClient {
    pub const BLOB_SIZE_LIMIT_IN_BYTES: usize = 2 * 1024 * 1024; // 2MB todo: add to config
    pub const STATUS_QUERY_TIMEOUT: u64 = 1800; // 30 minutes todo: add to config
    pub const STATUS_QUERY_RETRY_INTERVAL: u64 = 5; // 5 seconds todo: add to config
    pub const WAIT_FOR_FINALAZATION: bool = false; // todo: add to config
    pub async fn new(config: DisperserConfig) -> anyhow::Result<Self> {
        let inner = Channel::builder(config.disperser_rpc.parse()?)
            .tls_config(ClientTlsConfig::new().with_native_roots())?;
        Ok(Self {
            disperser: Arc::new(Mutex::new(DisperserClient::connect(inner).await?)),
            config,
        })
    }

    fn result_to_status(&self, result: i32) -> disperser::BlobStatus {
        match result {
            0 => disperser::BlobStatus::Unknown,
            1 => disperser::BlobStatus::Processing,
            2 => disperser::BlobStatus::Confirmed,
            3 => disperser::BlobStatus::Failed,
            4 => disperser::BlobStatus::Finalized,
            5 => disperser::BlobStatus::InsufficientSignatures,
            6 => disperser::BlobStatus::Dispersing,
            _ => disperser::BlobStatus::Unknown,
        }
    }

    pub async fn put_blob(&self, blob_data: Vec<u8>) -> Result<BlobInfo, ()> {
        let reply = self
            .disperser
            .lock()
            .await
            .disperse_blob(DisperseBlobRequest {
                data: kzgpad_rs::convert_by_padding_empty_byte(&blob_data),
                custom_quorum_numbers: self
                    .config
                    .custom_quorum_numbers
                    .clone()
                    .unwrap_or_default(),
                account_id: self.config.account_id.clone().unwrap_or_default(),
            })
            .await
            .unwrap()
            .into_inner();

        if self.result_to_status(reply.result) == disperser::BlobStatus::Failed {
            return Err(());
        }

        let mut interval = interval(Duration::from_secs(Self::STATUS_QUERY_RETRY_INTERVAL));
        let start_time = Instant::now();
        while Instant::now() - start_time < Duration::from_secs(Self::STATUS_QUERY_TIMEOUT) {
            let blob_status_reply = self
                .disperser
                .lock()
                .await
                .get_blob_status(BlobStatusRequest {
                    request_id: reply.request_id.clone(),
                })
                .await
                .unwrap()
                .into_inner();

            let blob_status = blob_status_reply.status();

            match blob_status {
                disperser::BlobStatus::Unknown => {
                    interval.tick().await;
                }
                disperser::BlobStatus::Processing => {
                    interval.tick().await;
                }
                disperser::BlobStatus::Confirmed => {
                    if Self::WAIT_FOR_FINALAZATION {
                        interval.tick().await;
                    } else {
                        match blob_status_reply.info {
                            Some(info) => {
                                return BlobInfo::try_from(info).map_err(|_| ());
                            }
                            None => {
                                return Err(());
                            }
                        }
                    }
                }
                disperser::BlobStatus::Failed => {
                    return Err(());
                }
                disperser::BlobStatus::InsufficientSignatures => {
                    return Err(());
                }
                disperser::BlobStatus::Dispersing => {
                    interval.tick().await;
                }
                disperser::BlobStatus::Finalized => match blob_status_reply.info {
                    Some(info) => {
                        return BlobInfo::try_from(info).map_err(|_| ());
                    }
                    None => {
                        return Err(());
                    }
                },
            }
        }

        return Err(());
    }

    pub async fn get_blob(
        &self,
        batch_header_hash: Vec<u8>,
        blob_index: u32,
    ) -> Result<Vec<u8>, ()> {
        let get_response = self
            .disperser
            .lock()
            .await
            .retrieve_blob(disperser::RetrieveBlobRequest {
                batch_header_hash: batch_header_hash,
                blob_index: blob_index,
            })
            .await
            .unwrap()
            .into_inner();

        if get_response.data.len() == 0 {
            return Err(());
        }

        return Ok(get_response.data);
    }
}
