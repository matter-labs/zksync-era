use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use anyhow::Error;
use rand::{rngs::OsRng, Rng, RngCore};
use sha3::{Digest, Keccak256};
use tokio::time::interval;
use zksync_config::configs::da_client::eigen::MemStoreConfig;
use zksync_da_client::types::{DAError, InclusionData};

use super::blob_info::{self, BlobInfo};

#[derive(Debug, PartialEq)]
pub enum MemStoreError {
    BlobToLarge,
    BlobAlreadyExists,
    IncorrectCommitment,
    #[cfg(test)]
    BlobNotFound,
}

impl From<MemStoreError> for Error {
    fn from(val: MemStoreError) -> Self {
        match val {
            MemStoreError::BlobToLarge => Error::msg("Blob too large"),
            MemStoreError::BlobAlreadyExists => Error::msg("Blob already exists"),
            MemStoreError::IncorrectCommitment => Error::msg("Incorrect commitment"),
            #[cfg(test)]
            MemStoreError::BlobNotFound => Error::msg("Blob not found"),
        }
    }
}

#[derive(Debug)]
struct MemStoreData {
    store: HashMap<String, Vec<u8>>,
    key_starts: HashMap<String, Instant>,
}

/// This struct represents a memory store for blobs.
/// It should be used for testing purposes only.
#[derive(Clone, Debug)]
pub struct MemStore {
    pub config: MemStoreConfig,
    data: Arc<RwLock<MemStoreData>>,
}

impl MemStore {
    pub fn new(config: MemStoreConfig) -> Arc<Self> {
        let memstore = Arc::new(Self {
            config,
            data: Arc::new(RwLock::new(MemStoreData {
                store: HashMap::new(),
                key_starts: HashMap::new(),
            })),
        });
        let store_clone = Arc::clone(&memstore);
        tokio::spawn(async move {
            store_clone.pruning_loop().await;
        });
        memstore
    }

    /// Saves a blob to the memory store, it harcodes the blob info, since we don't care about it in a memory based store
    pub async fn put_blob(self: Arc<Self>, value: Vec<u8>) -> Result<String, MemStoreError> {
        tokio::time::sleep(Duration::from_millis(self.config.put_latency)).await;
        if value.len() as u64 > self.config.max_blob_size_bytes {
            return Err(MemStoreError::BlobToLarge);
        }

        let mut entropy = [0u8; 10];
        OsRng.fill_bytes(&mut entropy);

        let mut hasher = Keccak256::new();
        hasher.update(entropy);
        let mock_batch_root = hasher.finalize().to_vec();

        let block_num = OsRng.gen_range(0u32..1000);

        let blob_info = blob_info::BlobInfo {
            blob_header: blob_info::BlobHeader {
                commitment: blob_info::G1Commitment {
                    // todo: generate real commitment
                    x: vec![0u8; 32],
                    y: vec![0u8; 32],
                },
                data_length: value.len() as u32,
                blob_quorum_params: vec![blob_info::BlobQuorumParam {
                    quorum_number: 1,
                    adversary_threshold_percentage: 29,
                    confirmation_threshold_percentage: 30,
                    chunk_length: 300,
                }],
            },
            blob_verification_proof: blob_info::BlobVerificationProof {
                batch_medatada: blob_info::BatchMetadata {
                    batch_header: blob_info::BatchHeader {
                        batch_root: mock_batch_root.clone(),
                        quorum_numbers: vec![0x1, 0x0],
                        quorum_signed_percentages: vec![0x60, 0x90],
                        reference_block_number: block_num,
                    },
                    signatory_record_hash: mock_batch_root,
                    fee: vec![],
                    confirmation_block_number: block_num,
                    batch_header_hash: vec![],
                },
                batch_id: 69,
                blob_index: 420,
                inclusion_proof: entropy.to_vec(),
                quorum_indexes: vec![0x1, 0x0],
            },
        };

        let cert_bytes = rlp::encode(&blob_info).to_vec();

        let key = String::from_utf8_lossy(
            blob_info
                .blob_verification_proof
                .inclusion_proof
                .clone()
                .as_slice(),
        )
        .to_string();

        let mut data = self.data.write().unwrap();

        if data.store.contains_key(key.as_str()) {
            return Err(MemStoreError::BlobAlreadyExists);
        }

        data.key_starts.insert(key.clone(), Instant::now());
        data.store.insert(key, value);
        Ok(hex::encode(cert_bytes))
    }

    /// It returns the inclusion proof
    pub async fn get_inclusion_data(
        self: Arc<Self>,
        blob_id: &str,
    ) -> anyhow::Result<Option<InclusionData>, DAError> {
        let rlp_encoded_bytes = hex::decode(blob_id).map_err(|_| DAError {
            error: MemStoreError::IncorrectCommitment.into(),
            is_retriable: false,
        })?;
        let blob_info: BlobInfo = rlp::decode(&rlp_encoded_bytes).map_err(|_| DAError {
            error: MemStoreError::IncorrectCommitment.into(),
            is_retriable: false,
        })?;
        let inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        Ok(Some(InclusionData {
            data: inclusion_data,
        }))
    }

    /// This function is only used on tests, it returns the blob data
    #[cfg(test)]
    pub async fn get_blob_data(
        self: Arc<Self>,
        blob_id: &str,
    ) -> anyhow::Result<Option<Vec<u8>>, DAError> {
        tokio::time::sleep(Duration::from_millis(self.config.get_latency)).await;
        let request_id = hex::decode(blob_id).map_err(|_| DAError {
            error: MemStoreError::IncorrectCommitment.into(),
            is_retriable: false,
        })?;
        let blob_info: BlobInfo = rlp::decode(&request_id).map_err(|_| DAError {
            error: MemStoreError::IncorrectCommitment.into(),
            is_retriable: false,
        })?;
        let key = String::from_utf8_lossy(
            blob_info
                .blob_verification_proof
                .inclusion_proof
                .clone()
                .as_slice(),
        )
        .to_string();

        let data = self.data.read().map_err(|_| DAError {
            error: MemStoreError::BlobNotFound.into(),
            is_retriable: false,
        })?;
        match data.store.get(&key) {
            Some(value) => Ok(Some(value.clone())),
            None => Err(DAError {
                error: MemStoreError::BlobNotFound.into(),
                is_retriable: false,
            }),
        }
    }

    /// After some time has passed, blobs are removed from the store
    async fn prune_expired(self: Arc<Self>) {
        let mut data = self.data.write().unwrap();
        let mut to_remove = vec![];
        for (key, start) in data.key_starts.iter() {
            if start.elapsed() > Duration::from_secs(self.config.blob_expiration) {
                to_remove.push(key.clone());
            }
        }
        for key in to_remove {
            data.store.remove(&key);
            data.key_starts.remove(&key);
        }
    }

    /// Loop used to prune expired blobs
    async fn pruning_loop(self: Arc<Self>) {
        let mut interval = interval(Duration::from_secs(self.config.blob_expiration));

        loop {
            interval.tick().await;
            let self_clone = Arc::clone(&self);
            self_clone.prune_expired().await;
        }
    }
}
