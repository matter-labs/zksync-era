use std::fmt;

use serde::Deserialize;
use tokio::time::{sleep, Duration};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::eth_sender::EthTxBlobSidecar;

use crate::l1_fetcher::types::ParseError;

/// `MAX_RETRIES` is the maximum number of retries on failed blob retrieval.
const MAX_RETRIES: u8 = 5;
/// The interval in seconds to wait before retrying to fetch a blob.
const FAILED_FETCH_RETRY_INTERVAL_S: u64 = 10;

#[derive(Deserialize)]
struct JsonResponse {
    data: String,
}

#[async_trait::async_trait]
pub trait BlobClient: 'static + fmt::Debug + Send + Sync {
    async fn get_blob(&self, kzg_commitment: &[u8]) -> Result<Vec<u8>, ParseError>;
}

#[derive(Debug)]
pub struct LocalDbBlobSource {
    pool: ConnectionPool<Core>,
}

impl LocalDbBlobSource {
    pub fn new(connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            pool: connection_pool,
        }
    }
}

#[async_trait::async_trait]
impl BlobClient for LocalDbBlobSource {
    async fn get_blob(&self, kzg_commitment: &[u8]) -> Result<Vec<u8>, ParseError> {
        let mut storage = self.pool.connection().await.unwrap();
        let mut id = 1;
        loop {
            let tx = storage.eth_sender_dal().get_eth_tx(id).await.unwrap();
            id += 1;
            if tx.is_none() {
                panic!("No tx found");
            }

            if let Some(blob_sidecar) = tx.unwrap().blob_sidecar {
                match blob_sidecar {
                    EthTxBlobSidecar::EthTxBlobSidecarV1(sidecar) => {
                        for blob in sidecar.blobs {
                            if blob.commitment == kzg_commitment {
                                return Ok(blob.blob);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct BlobHttpClient {
    url: String,
    client: reqwest::Client,
}

impl BlobHttpClient {
    pub fn new(url: &str) -> anyhow::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Accept",
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self {
            url: url.to_string(),
            client,
        })
    }
}

#[async_trait::async_trait]
impl BlobClient for BlobHttpClient {
    async fn get_blob(&self, kzg_commitment: &[u8]) -> Result<Vec<u8>, ParseError> {
        let full_url = format!("{}0x{}", self.url, hex::encode(kzg_commitment));
        for attempt in 1..=MAX_RETRIES {
            match self.client.get(&full_url).send().await {
                Ok(response) => match response.text().await {
                    Ok(text) => match get_blob_data(&text) {
                        Ok(data) => {
                            let plain = if let Some(p) = data.strip_prefix("0x") {
                                p
                            } else {
                                &data
                            };
                            return hex::decode(plain).map_err(|e| {
                                ParseError::BlobFormatError(plain.to_string(), e.to_string())
                            });
                        }
                        Err(e) => {
                            tracing::error!("failed parsing response of {full_url}");
                            return Err(e);
                        }
                    },
                    Err(e) => {
                        tracing::error!("attempt {}: {} failed: {:?}", attempt, full_url, e);
                        sleep(Duration::from_secs(FAILED_FETCH_RETRY_INTERVAL_S)).await;
                    }
                },
                Err(e) => {
                    tracing::error!("attempt {}: GET {} failed: {:?}", attempt, full_url, e);
                    sleep(Duration::from_secs(FAILED_FETCH_RETRY_INTERVAL_S)).await;
                }
            }
        }
        Err(ParseError::BlobStorageError(full_url))
    }
}

fn get_blob_data(json_str: &str) -> Result<String, ParseError> {
    match serde_json::from_str::<JsonResponse>(json_str) {
        Ok(data) => Ok(data.data),
        Err(e) => Err(ParseError::BlobFormatError(
            json_str.to_string(),
            e.to_string(),
        )),
    }
}
