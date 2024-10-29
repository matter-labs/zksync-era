use std::{collections::HashMap, convert::TryInto, fmt::Debug};

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use zksync_multivm::interface::{L1BatchEnv, SystemEnv};
use zksync_object_store::{_reexports::BoxedError, serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{
    basic_fri_types::Eip4844Blobs, block::L2BlockExecutionData, commitment::PubdataParams,
    witness_block_state::WitnessStorageState, L1BatchNumber, ProtocolVersionId, H256, U256,
};

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
pub struct WitnessInputMerklePaths {
    // Merkle paths and some auxiliary information for each read / write operation in a block.
    merkle_paths: Vec<StorageLogMetadata>,
    next_enumeration_index: u64,
}

impl StoredObject for WitnessInputMerklePaths {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("merkel_tree_paths_{key}.bin")
    }

    serialize_using_bincode!();
}

impl WitnessInputMerklePaths {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VMRunWitnessInputData {
    pub l1_batch_number: L1BatchNumber,
    pub used_bytecodes: HashMap<U256, Vec<[u8; 32]>>,
    pub initial_heap_content: Vec<(usize, U256)>,
    pub protocol_version: ProtocolVersionId,
    pub bootloader_code: Vec<[u8; 32]>,
    pub default_account_code_hash: U256,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evm_emulator_code_hash: Option<U256>,
    pub storage_refunds: Vec<u32>,
    pub pubdata_costs: Vec<i32>,
    pub witness_block_state: WitnessStorageState,
}

// skip_serializing_if for field evm_emulator_code_hash doesn't work fine with bincode,
// so we are implementing custom deserialization for it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMRunWitnessInputDataLegacy {
    pub l1_batch_number: L1BatchNumber,
    pub used_bytecodes: HashMap<U256, Vec<[u8; 32]>>,
    pub initial_heap_content: Vec<(usize, U256)>,
    pub protocol_version: ProtocolVersionId,
    pub bootloader_code: Vec<[u8; 32]>,
    pub default_account_code_hash: U256,
    pub storage_refunds: Vec<u32>,
    pub pubdata_costs: Vec<i32>,
    pub witness_block_state: WitnessStorageState,
}

impl From<VMRunWitnessInputDataLegacy> for VMRunWitnessInputData {
    fn from(value: VMRunWitnessInputDataLegacy) -> Self {
        Self {
            l1_batch_number: value.l1_batch_number,
            used_bytecodes: value.used_bytecodes,
            initial_heap_content: value.initial_heap_content,
            protocol_version: value.protocol_version,
            bootloader_code: value.bootloader_code,
            default_account_code_hash: value.default_account_code_hash,
            evm_emulator_code_hash: None,
            storage_refunds: value.storage_refunds,
            pubdata_costs: value.pubdata_costs,
            witness_block_state: value.witness_block_state,
        }
    }
}

impl StoredObject for VMRunWitnessInputData {
    const BUCKET: Bucket = Bucket::WitnessInput;

    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("vm_run_data_{key}.bin")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        zksync_object_store::bincode::serialize(self).map_err(Into::into)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        zksync_object_store::bincode::deserialize::<VMRunWitnessInputData>(&bytes).or_else(|_| {
            zksync_object_store::bincode::deserialize::<VMRunWitnessInputDataLegacy>(&bytes)
                .map(Into::into)
                .map_err(Into::into)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WitnessInputData {
    pub vm_run_data: VMRunWitnessInputData,
    pub merkle_paths: WitnessInputMerklePaths,
    pub previous_batch_metadata: L1BatchMetadataHashes,
    pub eip_4844_blobs: Eip4844Blobs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessInputDataLegacy {
    pub vm_run_data: VMRunWitnessInputDataLegacy,
    pub merkle_paths: WitnessInputMerklePaths,
    pub previous_batch_metadata: L1BatchMetadataHashes,
    pub eip_4844_blobs: Eip4844Blobs,
}

impl From<WitnessInputDataLegacy> for WitnessInputData {
    fn from(value: WitnessInputDataLegacy) -> Self {
        Self {
            vm_run_data: value.vm_run_data.into(),
            merkle_paths: value.merkle_paths,
            previous_batch_metadata: value.previous_batch_metadata,
            eip_4844_blobs: value.eip_4844_blobs,
        }
    }
}

impl StoredObject for WitnessInputData {
    const BUCKET: Bucket = Bucket::WitnessInput;

    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("witness_inputs_{key}.bin")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        zksync_object_store::bincode::serialize(self).map_err(Into::into)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        zksync_object_store::bincode::deserialize::<WitnessInputData>(&bytes).or_else(|_| {
            zksync_object_store::bincode::deserialize::<WitnessInputDataLegacy>(&bytes)
                .map(Into::into)
                .map_err(Into::into)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct L1BatchMetadataHashes {
    pub root_hash: H256,
    pub meta_hash: H256,
    pub aux_hash: H256,
}

/// Version 1 of the data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V1TeeVerifierInput {
    pub vm_run_data: VMRunWitnessInputData,
    pub merkle_paths: WitnessInputMerklePaths,
    pub l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
    pub pubdata_params: PubdataParams,
}

impl V1TeeVerifierInput {
    pub fn new(
        vm_run_data: VMRunWitnessInputData,
        merkle_paths: WitnessInputMerklePaths,
        l2_blocks_execution_data: Vec<L2BlockExecutionData>,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_params: PubdataParams,
    ) -> Self {
        V1TeeVerifierInput {
            vm_run_data,
            merkle_paths,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            pubdata_params,
        }
    }
}

/// Data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum TeeVerifierInput {
    /// `V0` suppresses warning about irrefutable `let...else` pattern
    V0,
    V1(V1TeeVerifierInput),
}

impl TeeVerifierInput {
    pub fn new(input: V1TeeVerifierInput) -> Self {
        TeeVerifierInput::V1(input)
    }
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

        let mut job = WitnessInputMerklePaths::new(4);
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
