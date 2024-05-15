use std::{convert::TryInto, fmt::Debug};

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{L1BatchNumber, H256, U256};

const HASH_LEN: usize = H256::len_bytes();

/// Metadata emitted by a Merkle tree after processing single storage log.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageLogMetadata {
    #[serde_as(as = "Bytes")]
    pub root_hash: [u8; HASH_LEN],
    pub is_write: bool,
    pub first_write: bool,
    #[serde_as(as = "Vec<Bytes>")]
    pub merkle_paths: Vec<[u8; HASH_LEN]>,
    pub leaf_hashed_key: U256,
    pub leaf_enumeration_index: u64,
    // **NB.** For compatibility reasons, `#[serde_as(as = "Bytes")]` attributes are not added below.
    pub value_written: [u8; HASH_LEN],
    pub value_read: [u8; HASH_LEN],
}

impl StorageLogMetadata {
    pub fn leaf_hashed_key_array(&self) -> [u8; 32] {
        let mut result = [0_u8; 32];
        self.leaf_hashed_key.to_little_endian(&mut result);
        result
    }

    pub fn into_merkle_paths_array<const PATH_LEN: usize>(self) -> Box<[[u8; HASH_LEN]; PATH_LEN]> {
        let actual_len = self.merkle_paths.len();
        self.merkle_paths.try_into().unwrap_or_else(|_| {
            panic!(
                "Unexpected length of Merkle paths in `StorageLogMetadata`: expected {}, got {}",
                PATH_LEN, actual_len
            );
        })
    }
}

/// Witness data produced by the Merkle tree as a result of processing a single block. Used
/// as an input to the witness generator.
///
/// # Stability
///
/// This type is serialized using `bincode` to be passed from the metadata calculator
/// to the witness generator. As such, changes in its `serde` serialization
/// must be backwards-compatible.
///
/// # Compact form
///
/// In order to reduce storage space, this job supports a compact format. In this format,
/// only the first item in `merkle_paths` is guaranteed to have the full Merkle path (i.e.,
/// 256 items with the current Merkle tree). The following items may have less hashes in their
/// Merkle paths; if this is the case, the starting hashes are skipped and are the same
/// as in the first path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrepareBasicCircuitsJob {
    // Merkle paths and some auxiliary information for each read / write operation in a block.
    merkle_paths: Vec<StorageLogMetadata>,
    next_enumeration_index: u64,
}

impl StoredObject for PrepareBasicCircuitsJob {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("merkel_tree_paths_{key}.bin")
    }

    serialize_using_bincode!();
}

impl PrepareBasicCircuitsJob {
    /// Creates a new job with the specified leaf index and no included paths.
    pub fn new(next_enumeration_index: u64) -> Self {
        Self {
            merkle_paths: vec![],
            next_enumeration_index,
        }
    }

    /// Returns the next leaf index at the beginning of the block.
    pub fn next_enumeration_index(&self) -> u64 {
        self.next_enumeration_index
    }

    /// Reserves additional capacity for Merkle paths.
    pub fn reserve(&mut self, additional_capacity: usize) {
        self.merkle_paths.reserve(additional_capacity);
    }

    /// Pushes an additional Merkle path.
    pub fn push_merkle_path(&mut self, mut path: StorageLogMetadata) {
        let Some(first_path) = self.merkle_paths.first() else {
            self.merkle_paths.push(path);
            return;
        };
        assert_eq!(first_path.merkle_paths.len(), path.merkle_paths.len());

        let mut hash_pairs = path.merkle_paths.iter().zip(&first_path.merkle_paths);
        let first_unique_idx =
            hash_pairs.position(|(hash, first_path_hash)| hash != first_path_hash);
        let first_unique_idx = first_unique_idx.unwrap_or(path.merkle_paths.len());
        path.merkle_paths = path.merkle_paths.split_off(first_unique_idx);
        self.merkle_paths.push(path);
    }

    /// Converts this job into an iterator over the contained Merkle paths.
    pub fn into_merkle_paths(self) -> impl ExactSizeIterator<Item = StorageLogMetadata> {
        let mut merkle_paths = self.merkle_paths;
        if let [first, rest @ ..] = merkle_paths.as_mut_slice() {
            for path in rest {
                assert!(
                    path.merkle_paths.len() <= first.merkle_paths.len(),
                    "Merkle paths in `PrepareBasicCircuitsJob` are malformed; the first path is not \
                     the longest one"
                );
                let spliced_len = first.merkle_paths.len() - path.merkle_paths.len();
                let spliced_hashes = &first.merkle_paths[0..spliced_len];
                path.merkle_paths
                    .splice(0..0, spliced_hashes.iter().cloned());
                debug_assert_eq!(path.merkle_paths.len(), first.merkle_paths.len());
            }
        }
        merkle_paths.into_iter()
    }
}

/// Enriched `PrepareBasicCircuitsJob`. All the other fields are taken from the `l1_batches` table.
#[derive(Debug, Clone)]
pub struct BasicCircuitWitnessGeneratorInput {
    pub block_number: L1BatchNumber,
    pub previous_block_hash: H256,
    pub previous_block_timestamp: u64,
    pub block_timestamp: u64,
    pub used_bytecodes_hashes: Vec<U256>,
    pub initial_heap_content: Vec<(usize, U256)>,
    pub merkle_paths_input: PrepareBasicCircuitsJob,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_basic_circuits_job_roundtrip() {
        let zero_hash = [0_u8; 32];
        let logs = (0..10).map(|i| {
            let mut merkle_paths = vec![zero_hash; 255];
            merkle_paths.push([i as u8; 32]);
            StorageLogMetadata {
                root_hash: zero_hash,
                is_write: i % 2 == 0,
                first_write: i % 3 == 0,
                merkle_paths,
                leaf_hashed_key: U256::from(i),
                leaf_enumeration_index: i + 1,
                value_written: [i as u8; 32],
                value_read: [0; 32],
            }
        });
        let logs: Vec<_> = logs.collect();

        let mut job = PrepareBasicCircuitsJob::new(4);
        job.reserve(logs.len());
        for log in &logs {
            job.push_merkle_path(log.clone());
        }

        // Check that Merkle paths are compacted.
        for (i, log) in job.merkle_paths.iter().enumerate() {
            let expected_merkle_path_len = if i == 0 { 256 } else { 1 };
            assert_eq!(log.merkle_paths.len(), expected_merkle_path_len);
        }

        let logs_from_job: Vec<_> = job.into_merkle_paths().collect();
        assert_eq!(logs_from_job, logs);
    }
}
