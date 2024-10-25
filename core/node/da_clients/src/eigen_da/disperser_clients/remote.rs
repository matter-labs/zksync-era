use std::{str::FromStr, sync::Arc};

use anyhow::anyhow;
use byteorder::{BigEndian, ByteOrder};
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};
use tokio::sync::Mutex;
use tonic::transport::Channel;
use zksync_config::configs::da_client::eigen_da::DisperserConfig;
use zksync_da_client::types::{self};

use super::disperser::{
    self, authenticated_reply::Payload, disperser_client::DisperserClient, AuthenticatedReply,
    AuthenticatedRequest, AuthenticationData, BlobStatus, BlobStatusRequest, DisperseBlobRequest,
};
use crate::eigen_da::client::{to_non_retriable_error, to_retriable_error};

#[derive(Clone, Debug)]
pub struct RemoteClient {
    pub disperser: Arc<Mutex<DisperserClient<Channel>>>,
    pub config: DisperserConfig,
}

fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(input);
    hasher.finalize(&mut output);
    output
}

fn sign(challenge: u32, private_key: &SecretKey) -> Vec<u8> {
    let mut buf = [0u8; 4];
    BigEndian::write_u32(&mut buf, challenge);
    let hash = keccak256(&buf);
    let message = Message::from_slice(&hash).unwrap();
    let secp = Secp256k1::signing_only();
    let recoverable_sig = secp.sign_ecdsa_recoverable(&message, private_key);

    // Step 5: Convert recoverable signature to a 65-byte array (64 bytes for signature + 1 byte for recovery ID)
    let (recovery_id, sig_bytes) = recoverable_sig.serialize_compact();

    // Step 6: Append the recovery ID as the last byte to form a 65-byte signature
    let mut full_signature = [0u8; 65];
    full_signature[..64].copy_from_slice(&sig_bytes);
    full_signature[64] = recovery_id.to_i32() as u8; // Append the recovery ID as the last byte

    full_signature.to_vec()
}

impl RemoteClient {
    async fn authentication(
        &self,
        blob_data: Vec<u8>,
        custom_quorum_numbers: Vec<u32>,
        account_id: String,
        private_key: &SecretKey,
    ) -> Result<disperser::DisperseBlobReply, anyhow::Error> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<AuthenticatedRequest>();
        let request = AuthenticatedRequest {
            payload: Some(disperser::authenticated_request::Payload::DisperseRequest(
                DisperseBlobRequest {
                    data: blob_data,
                    custom_quorum_numbers,
                    account_id,
                },
            )),
        };
        sender.send(request)?;
        let receiver_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(receiver);
        let mut stream = self
            .disperser
            .lock()
            .await
            .disperse_blob_authenticated(receiver_stream)
            .await?;
        let result = stream.get_mut().message().await?;

        let reply = if let Some(AuthenticatedReply {
            payload: Some(Payload::BlobAuthHeader(header)),
        }) = result
        {
            let challenge = header.challenge_parameter;
            let new_request = AuthenticatedRequest {
                payload: Some(
                    disperser::authenticated_request::Payload::AuthenticationData(
                        AuthenticationData {
                            authentication_data: sign(challenge, private_key),
                        },
                    ),
                ),
            };
            sender.send(new_request)?;
            let result = stream.get_mut().message().await?;

            let reply = if let Some(AuthenticatedReply {
                payload: Some(Payload::DisperseReply(reply)),
            }) = result
            {
                reply
            } else {
                return Err(anyhow!("Failed to authenticate"));
            };
            reply
        } else {
            return Err(anyhow!("Failed to authenticate"));
        };

        Ok(reply)
    }

    pub async fn disperse_blob(
        &self,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        let config = self.config.clone();
        let custom_quorum_numbers = config.custom_quorum_numbers.unwrap_or_default();
        let account_id = config.account_id.unwrap_or_default();
        let authenticated_dispersal = false; // config.authenticated_dispersal;
        match authenticated_dispersal {
            true => {
                let secp = Secp256k1::new();
                let secret_key = SecretKey::from_str(account_id.as_str()).unwrap();
                let public_key = PublicKey::from_secret_key(&secp, &secret_key);
                let account_id =
                    "0x".to_string() + &hex::encode(public_key.serialize_uncompressed());

                let request_id = self
                    .authentication(blob_data, custom_quorum_numbers, account_id, &secret_key)
                    .await
                    .unwrap()
                    .request_id;

                Ok(types::DispatchResponse {
                    blob_id: hex::encode(request_id),
                })
            }
            false => {
                let request_id = self
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
        }
    }

    pub async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<types::InclusionData>, types::DAError> {
        let request_id = hex::decode(blob_id).unwrap();
        let blob_status_reply = self
            .disperser
            .lock()
            .await
            .get_blob_status(BlobStatusRequest { request_id })
            .await
            .unwrap()
            .into_inner();
        let blob_status = blob_status_reply.status();
        match blob_status {
            BlobStatus::Unknown => Err(to_retriable_error(anyhow::anyhow!(
                "Blob status is unknown"
            ))),
            BlobStatus::Processing => Err(to_retriable_error(anyhow::anyhow!(
                "Blob is being processed"
            ))),
            BlobStatus::Confirmed => {
                if self.config.wait_for_finalization {
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
            BlobStatus::Failed => Err(to_non_retriable_error(anyhow::anyhow!("Blob has failed"))),
            BlobStatus::InsufficientSignatures => Err(to_non_retriable_error(anyhow::anyhow!(
                "Insufficient signatures for blob"
            ))),
            BlobStatus::Dispersing => Err(to_retriable_error(anyhow::anyhow!(
                "Blob is being dispersed"
            ))),
            BlobStatus::Finalized => Ok(Some(types::InclusionData {
                data: blob_status_reply
                    .info
                    .unwrap()
                    .blob_verification_proof
                    .unwrap()
                    .inclusion_proof,
            })),
        }
    }
}
