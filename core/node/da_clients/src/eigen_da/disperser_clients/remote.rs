use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::anyhow;
use byteorder::{BigEndian, ByteOrder};
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};
use tokio::{
    sync::Mutex,
    time::{interval, Instant},
};
use tonic::transport::Channel;
use zksync_config::configs::da_client::eigen_da::DisperserConfig;
use zksync_da_client::types::{self, DAError, DispatchResponse};

use super::{
    blob_info::BlobInfo,
    disperser::{
        self, authenticated_reply::Payload, disperser_client::DisperserClient, AuthenticatedReply,
        AuthenticatedRequest, AuthenticationData, BlobStatusRequest, DisperseBlobRequest,
    },
};

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

fn sign(challenge: u32, private_key: &SecretKey) -> Result<Vec<u8>, anyhow::Error> {
    let mut buf = [0u8; 4];
    BigEndian::write_u32(&mut buf, challenge);
    let hash = keccak256(&buf);
    let message = Message::from_slice(&hash)?;
    let secp = Secp256k1::signing_only();
    let recoverable_sig = secp.sign_ecdsa_recoverable(&message, private_key);

    // Step 5: Convert recoverable signature to a 65-byte array (64 bytes for signature + 1 byte for recovery ID)
    let (recovery_id, sig_bytes) = recoverable_sig.serialize_compact();

    // Step 6: Append the recovery ID as the last byte to form a 65-byte signature
    let mut full_signature = [0u8; 65];
    full_signature[..64].copy_from_slice(&sig_bytes);
    full_signature[64] = recovery_id.to_i32() as u8; // Append the recovery ID as the last byte

    Ok(full_signature.to_vec())
}

impl RemoteClient {
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
                            authentication_data: sign(challenge, private_key)?,
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

    async fn disperse_authenticated(
        &self,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        let secp = Secp256k1::new();
        let secret_key =
            SecretKey::from_str(self.config.account_id.clone().unwrap_or_default().as_str())
                .map_err(|_| DAError {
                    error: anyhow!("Failed to parse secret key"),
                    is_retriable: false,
                })?;
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let account_id = "0x".to_string() + &hex::encode(public_key.serialize_uncompressed());
        let custom_quorum_numbers = self
            .config
            .custom_quorum_numbers
            .clone()
            .unwrap_or_default();

        let reply = self
            .authentication(blob_data, custom_quorum_numbers, account_id, &secret_key)
            .await
            .map_err(|_| DAError {
                error: anyhow!("Failed to authenticate blob"),
                is_retriable: false,
            })?;

        if self.result_to_status(reply.result) == disperser::BlobStatus::Failed {
            return Err(DAError {
                error: anyhow!("Failed to disperse blob"),
                is_retriable: false,
            });
        }

        let mut interval = interval(Duration::from_secs(self.config.status_query_interval));
        let start_time = Instant::now();
        while Instant::now() - start_time < Duration::from_secs(self.config.status_query_timeout) {
            let blob_status_reply = self
                .disperser
                .lock()
                .await
                .get_blob_status(BlobStatusRequest {
                    request_id: reply.request_id.clone(),
                })
                .await
                .map_err(|e| DAError {
                    error: anyhow!("Failed to get blob status: {}", e),
                    is_retriable: true,
                })?
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
                    if self.config.wait_for_finalization {
                        interval.tick().await;
                    } else {
                        match blob_status_reply.info {
                            Some(info) => {
                                let blob_info = BlobInfo::try_from(info).map_err(|e| DAError {
                                    error: anyhow!(e),
                                    is_retriable: false,
                                })?;
                                let blob_id = hex::encode(rlp::encode(&blob_info).to_vec());
                                return Ok(DispatchResponse { blob_id });
                            }
                            None => {
                                return Err(DAError {
                                    error: anyhow!("Failed to get blob info"),
                                    is_retriable: false,
                                });
                            }
                        }
                    }
                }
                disperser::BlobStatus::Failed => {
                    return Err(DAError {
                        error: anyhow!("Failed to disperse blob"),
                        is_retriable: false,
                    });
                }
                disperser::BlobStatus::InsufficientSignatures => {
                    return Err(DAError {
                        error: anyhow!("Insufficient signatures"),
                        is_retriable: false,
                    });
                }
                disperser::BlobStatus::Dispersing => {
                    interval.tick().await;
                }
                disperser::BlobStatus::Finalized => match blob_status_reply.info {
                    Some(info) => {
                        let blob_info = BlobInfo::try_from(info).map_err(|e| DAError {
                            error: anyhow!(e),
                            is_retriable: false,
                        })?;
                        let blob_id = hex::encode(rlp::encode(&blob_info).to_vec());
                        return Ok(DispatchResponse { blob_id });
                    }
                    None => {
                        return Err(DAError {
                            error: anyhow!("Failed to get blob info"),
                            is_retriable: false,
                        });
                    }
                },
            }
        }

        return Err(DAError {
            error: anyhow!("Failed to disperse blob (timeout)"),
            is_retriable: false,
        });
    }

    async fn disperse_non_authenticated(
        &self,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        if blob_data.len() > self.config.blob_size_limit as usize {
            return Err(DAError {
                error: anyhow!("Blob too large"),
                is_retriable: false,
            });
        }

        let custom_quorum_numbers = self
            .config
            .custom_quorum_numbers
            .clone()
            .unwrap_or_default();
        let account_id = self.config.account_id.clone().unwrap_or_default();
        let reply = self
            .disperser
            .lock()
            .await
            .disperse_blob(DisperseBlobRequest {
                data: blob_data,
                custom_quorum_numbers,
                account_id,
            })
            .await
            .map_err(|e| DAError {
                error: anyhow!(e),
                is_retriable: true,
            })?
            .into_inner();

        if self.result_to_status(reply.result) == disperser::BlobStatus::Failed {
            return Err(DAError {
                error: anyhow!("Disperse failed"),
                is_retriable: true,
            });
        };

        let mut interval = interval(Duration::from_secs(self.config.status_query_interval));
        let start_time = Instant::now();
        while Instant::now() - start_time < Duration::from_secs(self.config.status_query_timeout) {
            let blob_status_reply = self
                .disperser
                .lock()
                .await
                .get_blob_status(BlobStatusRequest {
                    request_id: reply.request_id.clone(),
                })
                .await
                .map_err(|e| DAError {
                    error: anyhow!(e),
                    is_retriable: true,
                })?
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
                    if self.config.wait_for_finalization {
                        interval.tick().await;
                    } else {
                        match blob_status_reply.info {
                            Some(info) => {
                                let blob_info = BlobInfo::try_from(info).map_err(|e| DAError {
                                    error: anyhow!(e),
                                    is_retriable: false,
                                })?;
                                let blob_id = hex::encode(rlp::encode(&blob_info).to_vec());
                                return Ok(DispatchResponse { blob_id });
                            }
                            None => {
                                return Err(DAError {
                                    error: anyhow!("Failed to get blob info"),
                                    is_retriable: false,
                                });
                            }
                        }
                    }
                }
                disperser::BlobStatus::Failed => {
                    return Err(DAError {
                        error: anyhow!("Failed to disperse blob"),
                        is_retriable: false,
                    });
                }
                disperser::BlobStatus::InsufficientSignatures => {
                    return Err(DAError {
                        error: anyhow!("Insufficient signatures"),
                        is_retriable: false,
                    });
                }
                disperser::BlobStatus::Dispersing => {
                    interval.tick().await;
                }
                disperser::BlobStatus::Finalized => match blob_status_reply.info {
                    Some(info) => {
                        let blob_info = BlobInfo::try_from(info).map_err(|e| DAError {
                            error: anyhow!(e),
                            is_retriable: false,
                        })?;
                        let blob_id = hex::encode(rlp::encode(&blob_info).to_vec());
                        return Ok(DispatchResponse { blob_id });
                    }
                    None => {
                        return Err(DAError {
                            error: anyhow!("Failed to get blob info"),
                            is_retriable: false,
                        });
                    }
                },
            }
        }

        return Err(DAError {
            error: anyhow!("Timeout"),
            is_retriable: false,
        });
    }

    /// Returns a hex encoded string of the rlp encoded BlobInfo
    /// resulting from the dispersal of the blob data.
    pub async fn disperse_blob(
        &self,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        let blob_data = kzgpad_rs::convert_by_padding_empty_byte(&blob_data);
        match self.config.authenticaded {
            true => self.disperse_authenticated(blob_data).await,
            false => self.disperse_non_authenticated(blob_data).await,
        }
    }

    /// Receives a hex encoded string of the rlp encoded BlobInfo
    pub async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<types::InclusionData>, types::DAError> {
        let rlp_encoded_bytes = hex::decode(blob_id).map_err(|_| DAError {
            error: anyhow!("Failed to decode blob_id"),
            is_retriable: false,
        })?;
        let blob_info: BlobInfo = rlp::decode(&rlp_encoded_bytes).map_err(|_| DAError {
            error: anyhow!("Failed to decode blob_info"),
            is_retriable: false,
        })?;
        let inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        Ok(Some(types::InclusionData {
            data: inclusion_data,
        }))
    }

    #[cfg(test)]
    pub async fn get_blob_data(&self, blob_id: &str) -> anyhow::Result<Option<Vec<u8>>, DAError> {
        let commit = hex::decode(blob_id).map_err(|_| DAError {
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
            .disperser
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

        if get_response.data.len() == 0 {
            return Err(DAError {
                error: anyhow!("Failed to get blob data"),
                is_retriable: false,
            });
        }

        let data = kzgpad_rs::remove_empty_byte_from_padded_bytes(&get_response.data);
        return Ok(Some(data));
    }
}
