use std::{str::FromStr, sync::Arc};

use anyhow::Context;
use secp256k1::{ecdsa::RecoverableSignature, SecretKey};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use tonic::{
    transport::{Channel, ClientTlsConfig, Endpoint},
    Streaming,
};
use zksync_config::EigenConfig;
use zksync_web3_decl::client::{Client, DynClient, L1};

use super::{
    blob_info::BlobInfo,
    disperser::BlobInfo as DisperserBlobInfo,
    errors::{ConfigError, EigenClientError, EthClientError, VerificationError},
    verifier::Verifier,
    GetBlobData,
};
use crate::eigen::{
    blob_info,
    disperser::{
        self,
        authenticated_request::Payload::{AuthenticationData, DisperseRequest},
        disperser_client::DisperserClient,
        AuthenticatedReply, BlobAuthHeader,
    },
};

#[derive(Debug)]
pub(crate) struct RawEigenClient {
    client: DisperserClient<Channel>,
    private_key: SecretKey,
    pub config: EigenConfig,
    verifier: Verifier,
    blob_data_provider: Arc<dyn GetBlobData>,
}

pub(crate) const DATA_CHUNK_SIZE: usize = 32;

impl RawEigenClient {
    const BLOB_SIZE_LIMIT: usize = 1024 * 1024 * 2; // 2 MB

    pub async fn new(
        private_key: SecretKey,
        cfg: EigenConfig,
        get_blob_data: Arc<dyn GetBlobData>,
    ) -> Result<Self, EigenClientError> {
        let endpoint = Endpoint::from_str(cfg.disperser_rpc.as_str())
            .map_err(ConfigError::Tonic)?
            .tls_config(ClientTlsConfig::new())
            .map_err(ConfigError::Tonic)?;
        let client = DisperserClient::connect(endpoint)
            .await
            .map_err(ConfigError::Tonic)?;

        let rpc_url = cfg
            .eigenda_eth_rpc
            .clone()
            .ok_or(EthClientError::Rpc("EigenDA ETH RPC not set".to_string()))?;
        let query_client: Client<L1> = Client::http(rpc_url)
            .map_err(|e| EthClientError::Rpc(e.to_string()))?
            .build();
        let query_client = Box::new(query_client) as Box<DynClient<L1>>;

        let verifier = Verifier::new(cfg.clone(), Arc::new(query_client)).await?;
        Ok(RawEigenClient {
            client,
            private_key,
            config: cfg,
            verifier,
            blob_data_provider: get_blob_data,
        })
    }

    pub fn blob_size_limit() -> usize {
        Self::BLOB_SIZE_LIMIT
    }

    async fn dispatch_blob_non_authenticated(&self, data: Vec<u8>) -> anyhow::Result<String> {
        let padded_data = convert_by_padding_empty_byte(&data);
        let request = disperser::DisperseBlobRequest {
            data: padded_data,
            custom_quorum_numbers: vec![],
            account_id: String::default(), // Account Id is not used in non-authenticated mode
        };

        let disperse_reply = self
            .client
            .clone()
            .disperse_blob(request)
            .await?
            .into_inner();

        match disperser::BlobStatus::try_from(disperse_reply.result)? {
            disperser::BlobStatus::Failed
            | disperser::BlobStatus::InsufficientSignatures
            | disperser::BlobStatus::Unknown => Err(anyhow::anyhow!("Blob dispatch failed")),

            disperser::BlobStatus::Dispersing
            | disperser::BlobStatus::Processing
            | disperser::BlobStatus::Finalized
            | disperser::BlobStatus::Confirmed => Ok(hex::encode(disperse_reply.request_id)),
        }
    }

    async fn dispatch_blob_authenticated(&self, data: Vec<u8>) -> anyhow::Result<String> {
        let (tx, rx) = mpsc::unbounded_channel();

        // 1. send DisperseBlobRequest
        let padded_data = convert_by_padding_empty_byte(&data);
        self.disperse_data(padded_data, &tx)?;

        // this await is blocked until the first response on the stream, so we only await after sending the `DisperseBlobRequest`
        let mut response_stream = self
            .client
            .clone()
            .disperse_blob_authenticated(UnboundedReceiverStream::new(rx))
            .await?;
        let response_stream = response_stream.get_mut();

        // 2. receive BlobAuthHeader
        let blob_auth_header = self.receive_blob_auth_header(response_stream).await?;

        // 3. sign and send BlobAuthHeader
        self.submit_authentication_data(blob_auth_header.clone(), &tx)?;

        // 4. receive DisperseBlobReply
        let reply = response_stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("No response from server"))?
            .unwrap()
            .payload
            .ok_or_else(|| anyhow::anyhow!("No payload in response"))?;

        let disperser::authenticated_reply::Payload::DisperseReply(disperse_reply) = reply else {
            return Err(anyhow::anyhow!("Unexpected response from server"));
        };

        match disperser::BlobStatus::try_from(disperse_reply.result)? {
            disperser::BlobStatus::Failed
            | disperser::BlobStatus::InsufficientSignatures
            | disperser::BlobStatus::Unknown => Err(anyhow::anyhow!("Blob dispatch failed")),

            disperser::BlobStatus::Dispersing
            | disperser::BlobStatus::Processing
            | disperser::BlobStatus::Finalized
            | disperser::BlobStatus::Confirmed => Ok(hex::encode(disperse_reply.request_id)),
        }
    }

    pub async fn get_commitment(&self, request_id: &str) -> anyhow::Result<Option<BlobInfo>> {
        let blob_info = self.try_get_inclusion_data(request_id.to_string()).await?;

        let Some(blob_info) = blob_info else {
            return Ok(None);
        };
        let blob_info = blob_info::BlobInfo::try_from(blob_info)
            .map_err(|e| anyhow::anyhow!("Failed to convert blob info: {}", e))?;

        let data = self.get_blob_data(blob_info.clone()).await?;
        let data_db = self.blob_data_provider.get_blob_data(request_id).await?;
        if let Some(data_db) = data_db {
            if data_db != data {
                return Err(anyhow::anyhow!(
                    "Data from db and from disperser are different"
                ));
            }
        }
        self.verifier
            .verify_commitment(blob_info.blob_header.commitment.clone(), &data)
            .context("Failed to verify commitment")?;

        let result = self
            .verifier
            .verify_inclusion_data_against_settlement_layer(&blob_info)
            .await;
        if let Err(e) = result {
            match e {
                // in case of an error, the dispatcher will retry, so the need to return None
                VerificationError::EmptyHash => return Ok(None),
                _ => return Err(anyhow::anyhow!("Failed to verify inclusion data: {:?}", e)),
            }
        }

        tracing::info!("Blob dispatch confirmed, request id: {}", request_id);
        Ok(Some(blob_info))
    }

    pub async fn get_inclusion_data(&self, request_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let blob_info = self.get_commitment(request_id).await?;
        if let Some(blob_info) = blob_info {
            Ok(Some(blob_info.blob_verification_proof.inclusion_proof))
        } else {
            Ok(None)
        }
    }

    pub async fn dispatch_blob(&self, data: Vec<u8>) -> anyhow::Result<String> {
        if self.config.authenticated {
            self.dispatch_blob_authenticated(data).await
        } else {
            self.dispatch_blob_non_authenticated(data).await
        }
    }

    fn disperse_data(
        &self,
        data: Vec<u8>,
        tx: &mpsc::UnboundedSender<disperser::AuthenticatedRequest>,
    ) -> anyhow::Result<()> {
        let req = disperser::AuthenticatedRequest {
            payload: Some(DisperseRequest(disperser::DisperseBlobRequest {
                data,
                custom_quorum_numbers: vec![],
                account_id: get_account_id(&self.private_key),
            })),
        };

        tx.send(req)
            .map_err(|e| anyhow::anyhow!("Failed to send DisperseBlobRequest: {}", e))
    }

    fn submit_authentication_data(
        &self,
        blob_auth_header: BlobAuthHeader,
        tx: &mpsc::UnboundedSender<disperser::AuthenticatedRequest>,
    ) -> anyhow::Result<()> {
        // TODO: replace challenge_parameter with actual auth header when it is available
        let digest = zksync_basic_types::web3::keccak256(
            &blob_auth_header.challenge_parameter.to_be_bytes(),
        );
        let signature: RecoverableSignature = secp256k1::Secp256k1::signing_only()
            .sign_ecdsa_recoverable(
                &secp256k1::Message::from_slice(&digest[..])?,
                &self.private_key,
            );
        let (recovery_id, sig) = signature.serialize_compact();

        let mut signature = Vec::with_capacity(65);
        signature.extend_from_slice(&sig);
        signature.push(recovery_id.to_i32() as u8);

        let req = disperser::AuthenticatedRequest {
            payload: Some(AuthenticationData(disperser::AuthenticationData {
                authentication_data: signature,
            })),
        };

        tx.send(req)
            .map_err(|e| anyhow::anyhow!("Failed to send AuthenticationData: {}", e))
    }

    async fn receive_blob_auth_header(
        &self,
        response_stream: &mut Streaming<AuthenticatedReply>,
    ) -> anyhow::Result<disperser::BlobAuthHeader> {
        let reply = response_stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("No response from server"))?;

        let Ok(reply) = reply else {
            return Err(anyhow::anyhow!("Err from server: {:?}", reply));
        };

        let reply = reply
            .payload
            .ok_or_else(|| anyhow::anyhow!("No payload in response"))?;

        if let disperser::authenticated_reply::Payload::BlobAuthHeader(blob_auth_header) = reply {
            Ok(blob_auth_header)
        } else {
            Err(anyhow::anyhow!("Unexpected response from server"))
        }
    }

    async fn try_get_inclusion_data(
        &self,
        request_id: String,
    ) -> anyhow::Result<Option<DisperserBlobInfo>> {
        let polling_request = disperser::BlobStatusRequest {
            request_id: hex::decode(request_id)?,
        };

        let resp = self
            .client
            .clone()
            .get_blob_status(polling_request)
            .await?
            .into_inner();

        match disperser::BlobStatus::try_from(resp.status)? {
            disperser::BlobStatus::Processing | disperser::BlobStatus::Dispersing => Ok(None),
            disperser::BlobStatus::Failed => anyhow::bail!("Blob dispatch failed"),
            disperser::BlobStatus::InsufficientSignatures => {
                anyhow::bail!("Insufficient signatures")
            }
            disperser::BlobStatus::Confirmed => {
                if !self.config.wait_for_finalization {
                    let blob_info = resp.info.context("No blob header in response")?;
                    return Ok(Some(blob_info));
                }
                Ok(None)
            }
            disperser::BlobStatus::Finalized => {
                let blob_info = resp.info.context("No blob header in response")?;
                Ok(Some(blob_info))
            }
            _ => anyhow::bail!("Received unknown blob status"),
        }
    }

    pub async fn get_blob_data(&self, blob_info: BlobInfo) -> anyhow::Result<Vec<u8>> {
        use anyhow::anyhow;

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
            .await?
            .into_inner();

        if get_response.data.is_empty() {
            return Err(anyhow!("Failed to get blob data"));
        }

        let data = remove_empty_byte_from_padded_bytes(&get_response.data);
        Ok(data)
    }
}

fn get_account_id(secret_key: &SecretKey) -> String {
    let public_key =
        secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), secret_key);
    let hex = hex::encode(public_key.serialize_uncompressed());

    format!("0x{}", hex)
}

fn convert_by_padding_empty_byte(data: &[u8]) -> Vec<u8> {
    let parse_size = DATA_CHUNK_SIZE - 1;

    let chunk_count = data.len().div_ceil(parse_size);
    let mut valid_data = Vec::with_capacity(data.len() + chunk_count);

    for chunk in data.chunks(parse_size) {
        valid_data.push(0x00); // Add the padding byte (0x00)
        valid_data.extend_from_slice(chunk);
    }

    valid_data
}

fn remove_empty_byte_from_padded_bytes(data: &[u8]) -> Vec<u8> {
    let parse_size = DATA_CHUNK_SIZE;

    let chunk_count = data.len().div_ceil(parse_size);
    // Safe subtraction, as we know chunk_count is always less than the length of the data
    let mut valid_data = Vec::with_capacity(data.len() - chunk_count);

    for chunk in data.chunks(parse_size) {
        valid_data.extend_from_slice(&chunk[1..]);
    }

    valid_data
}

#[cfg(test)]
mod test {
    #[test]
    fn test_pad_and_unpad() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        let padded_data = super::convert_by_padding_empty_byte(&data);
        let unpadded_data = super::remove_empty_byte_from_padded_bytes(&padded_data);
        assert_eq!(data, unpadded_data);
    }

    #[test]
    fn test_pad_and_unpad_large() {
        let data = vec![1; 1000];
        let padded_data = super::convert_by_padding_empty_byte(&data);
        let unpadded_data = super::remove_empty_byte_from_padded_bytes(&padded_data);
        assert_eq!(data, unpadded_data);
    }

    #[test]
    fn test_pad_and_unpad_empty() {
        let data = Vec::new();
        let padded_data = super::convert_by_padding_empty_byte(&data);
        let unpadded_data = super::remove_empty_byte_from_padded_bytes(&padded_data);
        assert_eq!(data, unpadded_data);
    }
}
