use std::str::FromStr;

use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

use crate::{
    blob_info::BlobInfo,
    generated::{disperser, disperser::disperser_client::DisperserClient},
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

    pub async fn get_blob_data(&self, blob_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let commit = hex::decode(blob_id)?;

        let blob_info: BlobInfo = rlp::decode(&commit)?;
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

        if get_response.data.len() == 0 {
            panic!("Empty data returned from Disperser")
        }

        let data = kzgpad_rs::remove_empty_byte_from_padded_bytes(&get_response.data);
        return Ok(Some(data));
    }
}
