use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use rand::{rngs::OsRng, Rng, RngCore};
use rlp::decode;
use sha3::{Digest, Keccak256};
use tokio::time::interval;

use crate::{
    blob_info::{self, BlobInfo},
    errors::MemStoreError,
};

struct MemStoreConfig {
    max_blob_size_bytes: u64,
    blob_expiration: Duration,
    put_latency: Duration,
    get_latency: Duration,
}

struct MemStoreData {
    store: HashMap<String, Vec<u8>>,
    key_starts: HashMap<String, Instant>,
}

struct MemStore {
    config: MemStoreConfig,
    data: Arc<RwLock<MemStoreData>>,
}

impl MemStore {
    fn new(config: MemStoreConfig) -> Arc<Self> {
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

    async fn put(self: Arc<Self>, value: Vec<u8>) -> Result<Vec<u8>, MemStoreError> {
        tokio::time::sleep(self.config.put_latency).await;
        if value.len() as u64 > self.config.max_blob_size_bytes {
            return Err(MemStoreError::BlobToLarge.into());
        }

        // todo: Encode blob?

        let mut entropy = [0u8; 10];
        OsRng.fill_bytes(&mut entropy);

        let mut hasher = Keccak256::new();
        hasher.update(&entropy);
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
        Ok(cert_bytes)
    }

    async fn get(self: Arc<Self>, commit: Vec<u8>) -> Result<Vec<u8>, MemStoreError> {
        tokio::time::sleep(self.config.get_latency).await;
        let blob_info: BlobInfo =
            decode(&commit).map_err(|_| MemStoreError::IncorrectCommitment)?;
        let key = String::from_utf8_lossy(
            blob_info
                .blob_verification_proof
                .inclusion_proof
                .clone()
                .as_slice(),
        )
        .to_string();

        let data = self.data.read().unwrap();
        match data.store.get(&key) {
            Some(value) => Ok(value.clone()),
            None => Err(MemStoreError::BlobNotFound),
        }
        // TODO: verify commitment?
        // TODO: decode blob?
    }

    async fn prune_expired(self: Arc<Self>) {
        let mut data = self.data.write().unwrap();
        let mut to_remove = vec![];
        for (key, start) in data.key_starts.iter() {
            if start.elapsed() > self.config.blob_expiration {
                to_remove.push(key.clone());
            }
        }
        for key in to_remove {
            data.store.remove(&key);
            data.key_starts.remove(&key);
        }
    }

    async fn pruning_loop(self: Arc<Self>) {
        let mut interval = interval(self.config.blob_expiration);

        loop {
            interval.tick().await;
            let self_clone = Arc::clone(&self);
            self_clone.prune_expired().await;
        }
    }
}

mod test {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_memstore() {
        let config = MemStoreConfig {
            max_blob_size_bytes: 1024,
            blob_expiration: Duration::from_secs(60),
            put_latency: Duration::from_millis(100),
            get_latency: Duration::from_millis(100),
        };
        let store = MemStore::new(config);

        let blob = vec![0u8; 100];
        let cert = store.clone().put(blob.clone()).await.unwrap();
        let blob2 = store.get(cert).await.unwrap();
        assert_eq!(blob, blob2);
    }

    #[tokio::test]
    async fn test_memstore_multiple() {
        let config = MemStoreConfig {
            max_blob_size_bytes: 1024,
            blob_expiration: Duration::from_secs(60),
            put_latency: Duration::from_millis(100),
            get_latency: Duration::from_millis(100),
        };
        let store = MemStore::new(config);

        let blob = vec![0u8; 100];
        let blob2 = vec![1u8; 100];
        let cert = store.clone().put(blob.clone()).await.unwrap();
        let cert2 = store.clone().put(blob2.clone()).await.unwrap();
        let blob_result = store.clone().get(cert).await.unwrap();
        let blob_result2 = store.get(cert2).await.unwrap();
        assert_eq!(blob, blob_result);
        assert_eq!(blob2, blob_result2);
    }

    #[tokio::test]
    async fn test_memstore_latency() {
        let put_latency = Duration::from_millis(1000);
        let get_latency = Duration::from_millis(1000);
        let config = MemStoreConfig {
            max_blob_size_bytes: 1024,
            blob_expiration: Duration::from_secs(60),
            put_latency,
            get_latency,
        };
        let store = MemStore::new(config);

        let blob = vec![0u8; 100];
        let time_before_put = Instant::now();
        let cert = store.clone().put(blob.clone()).await.unwrap();
        assert!(time_before_put.elapsed() >= put_latency);
        let time_before_get = Instant::now();
        let blob2 = store.get(cert).await.unwrap();
        assert!(time_before_get.elapsed() >= get_latency);
        assert_eq!(blob, blob2);
    }

    #[tokio::test]
    async fn test_memstore_expiration() {
        let blob_expiration = Duration::from_millis(100);
        let config = MemStoreConfig {
            max_blob_size_bytes: 1024,
            blob_expiration,
            put_latency: Duration::from_millis(1),
            get_latency: Duration::from_millis(1),
        };
        let store = MemStore::new(config);

        let blob = vec![0u8; 100];
        let cert = store.clone().put(blob.clone()).await.unwrap();
        tokio::time::sleep(blob_expiration * 2).await;
        let result = store.get(cert).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), MemStoreError::BlobNotFound);
    }
}
