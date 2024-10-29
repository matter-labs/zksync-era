use std::str::FromStr;

use serde::Deserialize;
use tokio::time::{sleep, Duration};
use zksync_basic_types::url::SensitiveUrl;
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

pub struct BlobHttpClient {
    client: reqwest::Client,
    url_base: String,
}

impl BlobHttpClient {
    pub fn new(blob_url: String) -> anyhow::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Accept",
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self {
            client,
            url_base: blob_url,
        })
    }

    async fn get_blob_from_db(&self, kzg_commitment: &[u8]) -> Result<Vec<u8>, ParseError> {
        let pool = ConnectionPool::<Core>::singleton(
            SensitiveUrl::from_str(
                "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era",
            )
            .expect("Failed to parse the database URL"),
        )
        .build()
        .await
        .unwrap();
        let mut storage = pool.connection().await.unwrap();
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
    pub async fn get_blob(&self, kzg_commitment: &[u8]) -> Result<Vec<u8>, ParseError> {
        if self.url_base == "LOCAL_BLOBS_ONLY!" {
            return self.get_blob_from_db(kzg_commitment).await;
        }
        let url = self.format_url(kzg_commitment);
        for attempt in 1..=MAX_RETRIES {
            match self.client.get(&url).send().await {
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
                            tracing::error!("failed parsing response of {url}");
                            return Err(e);
                        }
                    },
                    Err(e) => {
                        tracing::error!("attempt {}: {} failed: {:?}", attempt, url, e);
                        sleep(Duration::from_secs(FAILED_FETCH_RETRY_INTERVAL_S)).await;
                    }
                },
                Err(e) => {
                    tracing::error!("attempt {}: GET {} failed: {:?}", attempt, url, e);
                    sleep(Duration::from_secs(FAILED_FETCH_RETRY_INTERVAL_S)).await;
                }
            }
        }
        Err(ParseError::BlobStorageError(url))
    }

    fn format_url(&self, kzg_commitment: &[u8]) -> String {
        format!("{}0x{}", self.url_base, hex::encode(kzg_commitment))
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
