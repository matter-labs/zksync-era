use std::str::FromStr;

use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

use crate::{
    blob_info::BlobInfo,
    generated::disperser::{self, disperser_client::DisperserClient},
};

#[derive(Debug, Clone)]
pub struct EigenClientRetriever {
    client: DisperserClient<Channel>,
}

impl EigenClientRetriever {
    pub async fn new(disperser_rpc: &str) -> anyhow::Result<Self> {
        let endpoint = Endpoint::from_str(disperser_rpc)?.tls_config(ClientTlsConfig::new())?;
        let client = DisperserClient::connect(endpoint)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Disperser server: {}", e))?;

        Ok(EigenClientRetriever { client })
    }

    pub async fn get_blob_data(&self, blob_info: BlobInfo) -> anyhow::Result<Option<Vec<u8>>> {
        let blob_index = blob_info.blob_verification_proof.blob_index;
        let batch_header_hash = blob_info
            .blob_verification_proof
            .batch_medatada
            .batch_header_hash;
        let get_response = self
            .client
            .clone()
            .retrieve_blob(disperser::RetrieveBlobRequest {
                batch_header_hash,
                blob_index,
            })
            .await
            .unwrap()
            .into_inner();

        if get_response.data.is_empty() {
            panic!("Empty data returned from Disperser")
        }

        let data = kzgpad_rs::remove_empty_byte_from_padded_bytes(&get_response.data);
        Ok(Some(data))
    }

    pub async fn get_blob_status(&self, blob_id: &str) -> anyhow::Result<Option<BlobInfo>> {
        let polling_request = disperser::BlobStatusRequest {
            request_id: hex::decode(blob_id)?,
        };

        let resp = self
            .client
            .clone()
            .get_blob_status(polling_request.clone())
            .await?
            .into_inner();

        match disperser::BlobStatus::try_from(resp.status)? {
            disperser::BlobStatus::Processing | disperser::BlobStatus::Dispersing => Ok(None),
            disperser::BlobStatus::Failed => Err(anyhow::anyhow!("Blob dispatch failed")),
            disperser::BlobStatus::InsufficientSignatures => {
                Err(anyhow::anyhow!("Insufficient signatures"))
            }
            disperser::BlobStatus::Confirmed => {
                let blob_info = resp
                    .info
                    .ok_or_else(|| anyhow::anyhow!("No blob header in response"))?;
                Ok(Some(blob_info.try_into().unwrap()))
            }
            disperser::BlobStatus::Finalized => {
                let blob_info = resp
                    .info
                    .ok_or_else(|| anyhow::anyhow!("No blob header in response"))?;
                Ok(Some(blob_info.try_into().unwrap()))
            }

            _ => Err(anyhow::anyhow!("Received unknown blob status")),
        }
    }
}
