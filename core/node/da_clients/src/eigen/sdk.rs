use std::{str::FromStr, sync::Arc, time::Duration};

use backon::{ConstantBuilder, Retryable};
use secp256k1::{ecdsa::RecoverableSignature, SecretKey};
use tokio::{
    sync::{mpsc, Mutex},
    time::Instant,
};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
use tonic::{
    transport::{Channel, ClientTlsConfig, Endpoint},
    Streaming,
};
use zksync_config::EigenConfig;
use zksync_da_client::types::DAError;
use zksync_eth_client::clients::PKSigningClient;
use zksync_types::{url::SensitiveUrl, K256PrivateKey, SLChainId, H160};
use zksync_web3_decl::client::{Client, DynClient, L1};

use super::{
    blob_info::BlobInfo,
    disperser::BlobInfo as DisperserBlobInfo,
    verifier::{Verifier, VerifierConfig},
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

#[derive(Debug, Clone)]
pub(crate) struct RawEigenClient {
    client: Arc<Mutex<DisperserClient<Channel>>>,
    private_key: SecretKey,
    pub config: EigenConfig,
    verifier: Verifier,
}

pub(crate) const DATA_CHUNK_SIZE: usize = 32;
pub(crate) const AVG_BLOCK_TIME: u64 = 12;

impl RawEigenClient {
    const BLOB_SIZE_LIMIT: usize = 1024 * 1024 * 2; // 2 MB

    pub async fn new(private_key: SecretKey, config: EigenConfig) -> anyhow::Result<Self> {
        let endpoint =
            Endpoint::from_str(config.disperser_rpc.as_str())?.tls_config(ClientTlsConfig::new())?;
        let client = Arc::new(Mutex::new(DisperserClient::connect(endpoint).await?));

        let verifier_config = VerifierConfig {
            rpc_url: config.eigenda_eth_rpc.clone(),
            svc_manager_addr: config.eigenda_svc_manager_address.clone(),
            max_blob_size: Self::BLOB_SIZE_LIMIT as u32,
            points: config.points_source.clone(),
            settlement_layer_confirmation_depth: config.settlement_layer_confirmation_depth.max(0)
                as u32,
            private_key: hex::encode(private_key.secret_bytes()),
            chain_id: config.chain_id,
        };

        let url = SensitiveUrl::from_str(&verifier_config.rpc_url)?;
        let query_client: Client<L1> = Client::http(url)?.build();
        let query_client = Box::new(query_client) as Box<DynClient<L1>>;
        let signing_client = PKSigningClient::new_raw(
            K256PrivateKey::from_bytes(zksync_types::H256::from_str(
                &verifier_config.private_key,
            )?)?,
            H160::from_str(&verifier_config.svc_manager_addr)?,
            Verifier::DEFAULT_PRIORITY_FEE_PER_GAS,
            SLChainId(verifier_config.chain_id),
            query_client,
        );

        let verifier = Verifier::new(verifier_config, signing_client)
            .await
            .map_err(|e| anyhow::anyhow!(format!("Failed to create verifier {:?}", e)))?;
        Ok(RawEigenClient {
            client,
            private_key,
            config,
            verifier,
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
            .lock()
            .await
            .disperse_blob(request)
            .await?
            .into_inner();

        Ok(hex::encode(disperse_reply.request_id))
    }

    async fn perform_verification(
        &self,
        blob_info: BlobInfo,
        disperse_elapsed: Duration,
    ) -> anyhow::Result<()> {
        (|| async { self.verifier.verify_certificate(blob_info.clone()).await })
            .retry(
                &ConstantBuilder::default()
                    .with_delay(Duration::from_secs(AVG_BLOCK_TIME))
                    .with_max_times(
                        (self.config.status_query_timeout
                            - disperse_elapsed.as_millis() as u64 / AVG_BLOCK_TIME)
                            as usize,
                    ),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Failed to verify certificate"))
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
            .lock()
            .await
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
        Ok(hex::encode(disperse_reply.request_id))
    }

    pub async fn get_inclusion_data(&self, blob_id: &str) -> anyhow::Result<String> {
        let disperse_time = Instant::now();
        let blob_info = self.await_for_inclusion(blob_id.to_string()).await?;

        let blob_info = blob_info::BlobInfo::try_from(blob_info)
            .map_err(|e| anyhow::anyhow!("Failed to convert blob info: {}", e))?;

        let disperse_elapsed = Instant::now() - disperse_time;
        let data = self
            .get_blob_data(&hex::encode(rlp::encode(&blob_info)))
            .await?;
        if data.is_none() {
            return Err(anyhow::anyhow!("Failed to get blob data"));
        }
        self.verifier
            .verify_commitment(blob_info.blob_header.commitment.clone(), data.unwrap())
            .map_err(|_| anyhow::anyhow!("Failed to verify commitment"))?;

        self.perform_verification(blob_info.clone(), disperse_elapsed)
            .await?;

        let verification_proof = blob_info.blob_verification_proof.clone();
        let blob_id = format!(
            "{}:{}",
            verification_proof.batch_id, verification_proof.blob_index
        );
        tracing::info!("Blob dispatch confirmed, blob id: {}", blob_id);
        Ok(hex::encode(rlp::encode(&blob_info)))
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

    async fn await_for_inclusion(&self, request_id: String) -> anyhow::Result<DisperserBlobInfo> {
        let polling_request = disperser::BlobStatusRequest {
            request_id: hex::decode(request_id)?,
        };

        let blob_info = (|| async {
            let resp = self
                .client
                .lock()
                .await
                .get_blob_status(polling_request.clone())
                .await?
                .into_inner();

            match disperser::BlobStatus::try_from(resp.status)? {
                disperser::BlobStatus::Processing | disperser::BlobStatus::Dispersing => {
                    Err(anyhow::anyhow!("Blob is still processing"))
                }
                disperser::BlobStatus::Failed => Err(anyhow::anyhow!("Blob dispatch failed")),
                disperser::BlobStatus::InsufficientSignatures => {
                    Err(anyhow::anyhow!("Insufficient signatures"))
                }
                disperser::BlobStatus::Confirmed => {
                    if !self.config.wait_for_finalization {
                        let blob_info = resp
                            .info
                            .ok_or_else(|| anyhow::anyhow!("No blob header in response"))?;
                        return Ok(blob_info);
                    }
                    Err(anyhow::anyhow!("Blob is still processing"))
                }
                disperser::BlobStatus::Finalized => {
                    let blob_info = resp
                        .info
                        .ok_or_else(|| anyhow::anyhow!("No blob header in response"))?;
                    Ok(blob_info)
                }

                _ => Err(anyhow::anyhow!("Received unknown blob status")),
            }
        })
        .retry(
            &ConstantBuilder::default()
                .with_delay(Duration::from_millis(self.config.status_query_interval))
                .with_max_times(
                    (self.config.status_query_timeout / self.config.status_query_interval) as usize,
                ),
        )
        .when(|e| e.to_string().contains("Blob is still processing"))
        .await?;

        Ok(blob_info)
    }

    pub async fn get_blob_data(&self, blob_info: &str) -> anyhow::Result<Option<Vec<u8>>, DAError> {
        use anyhow::anyhow;
        use zksync_da_client::types::DAError;

        use crate::eigen::blob_info::BlobInfo;

        let commit = hex::decode(blob_info).map_err(|_| DAError {
            error: anyhow!("Failed to decode blob_id"),
            is_retriable: false,
        })?;
        let blob_info: BlobInfo = rlp::decode(&commit).map_err(|_| DAError {
            error: anyhow!("Failed to decode blob_info"),
            is_retriable: false,
        })?;
        let blob_index = blob_info.blob_verification_proof.blob_index;
        let batch_header_hash = blob_info
            .blob_verification_proof
            .batch_medatada
            .batch_header_hash;
        let get_response = self
            .client
            .lock()
            .await
            .retrieve_blob(disperser::RetrieveBlobRequest {
                batch_header_hash,
                blob_index,
            })
            .await
            .map_err(|e| DAError {
                error: anyhow!(e),
                is_retriable: true,
            })?
            .into_inner();

        if get_response.data.is_empty() {
            return Err(DAError {
                error: anyhow!("Failed to get blob data"),
                is_retriable: false,
            });
        }

        let data = remove_empty_byte_from_padded_bytes(&get_response.data);
        Ok(Some(data))
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

    // Calculate the number of chunks
    let data_len = (data.len() + parse_size - 1) / parse_size;

    // Pre-allocate `valid_data` with enough space for all chunks
    let mut valid_data = vec![0u8; data_len * DATA_CHUNK_SIZE];
    let mut valid_end = data_len * DATA_CHUNK_SIZE;

    for (i, chunk) in data.chunks(parse_size).enumerate() {
        let offset = i * DATA_CHUNK_SIZE;
        valid_data[offset] = 0x00; // Set first byte of each chunk to 0x00 for big-endian compliance

        let copy_end = offset + 1 + chunk.len();
        valid_data[offset + 1..copy_end].copy_from_slice(chunk);

        if i == data_len - 1 && chunk.len() < parse_size {
            valid_end = offset + 1 + chunk.len();
        }
    }

    valid_data.truncate(valid_end);
    valid_data
}

fn remove_empty_byte_from_padded_bytes(data: &[u8]) -> Vec<u8> {
    let parse_size = DATA_CHUNK_SIZE;

    // Calculate the number of chunks
    let data_len = (data.len() + parse_size - 1) / parse_size;

    // Pre-allocate `valid_data` with enough space for all chunks
    let mut valid_data = vec![0u8; data_len * (DATA_CHUNK_SIZE - 1)];
    let mut valid_end = data_len * (DATA_CHUNK_SIZE - 1);

    for (i, chunk) in data.chunks(parse_size).enumerate() {
        let offset = i * (DATA_CHUNK_SIZE - 1);

        let copy_end = offset + chunk.len() - 1;
        valid_data[offset..copy_end].copy_from_slice(&chunk[1..]);

        if i == data_len - 1 && chunk.len() < parse_size {
            valid_end = offset + chunk.len() - 1;
        }
    }

    valid_data.truncate(valid_end);
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
