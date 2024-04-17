//! Data structures that have more metadata than their primary versions declared in this crate.
//! For example, L1 batch defined here has the `root_hash` field which is absent in `L1BatchHeader`.
//!
//! Existence of this module is caused by the execution model of zkSync: when executing transactions,
//! we aim to avoid expensive operations like the state root hash recalculation. State root hash is not
//! required for the rollup to execute L1 batches, it's needed for the proof generation and the Ethereum
//! transactions, thus the calculations are done separately and asynchronously.

use std::{collections::HashMap, convert::TryFrom};

use serde::{Deserialize, Serialize};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_system_constants::{
    KNOWN_CODES_STORAGE_ADDRESS, L2_TO_L1_LOGS_TREE_ROOT_KEY, STATE_DIFF_HASH_KEY,
    ZKPORTER_IS_AVAILABLE,
};
use zksync_utils::u256_to_h256;

use crate::{
    blob::num_blobs_required,
    block::{L1BatchHeader, L1BatchTreeData},
    l2_to_l1_log::{
        l2_to_l1_logs_tree_size, parse_system_logs_for_blob_hashes, L2ToL1Log, SystemL2ToL1Log,
        UserL2ToL1Log,
    },
    web3::signing::keccak256,
    writes::{
        compress_state_diffs, InitialStorageWrite, RepeatedStorageWrite, StateDiffRecord,
        PADDED_ENCODED_STORAGE_DIFF_LEN_BYTES,
    },
    ProtocolVersionId, H256,
};

#[cfg(test)]
mod tests;

/// Type that can be serialized for commitment.
pub trait SerializeCommitment {
    /// Size of the structure in bytes.
    const SERIALIZED_SIZE: usize;
    /// Serializes this struct into the provided buffer, which is guaranteed to have byte length
    /// [`Self::SERIALIZED_SIZE`].
    fn serialize_commitment(&self, buffer: &mut [u8]);
}

/// Serialize elements for commitment. The results consist of:
/// 1. Number of elements (4 bytes)
/// 2. Serialized elements
pub fn pre_boojum_serialize_commitments<I: SerializeCommitment>(values: &[I]) -> Vec<u8> {
    let final_len = values.len() * I::SERIALIZED_SIZE + 4;
    let mut input = vec![0_u8; final_len];
    input[0..4].copy_from_slice(&(values.len() as u32).to_be_bytes());

    let chunks = input[4..].chunks_mut(I::SERIALIZED_SIZE);
    for (value, chunk) in values.iter().zip(chunks) {
        value.serialize_commitment(chunk);
    }
    input
}

/// Serialize elements for commitment. The result consists of packed serialized elements.
pub fn serialize_commitments<I: SerializeCommitment>(values: &[I]) -> Vec<u8> {
    let final_len = values.len() * I::SERIALIZED_SIZE;
    let mut input = vec![0_u8; final_len];

    let chunks = input.chunks_mut(I::SERIALIZED_SIZE);
    for (value, chunk) in values.iter().zip(chunks) {
        value.serialize_commitment(chunk);
    }
    input
}

/// Precalculated data for the L1 batch that was used in commitment and L1 transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchMetadata {
    pub root_hash: H256,
    pub rollup_last_leaf_index: u64,
    pub initial_writes_compressed: Option<Vec<u8>>,
    pub repeated_writes_compressed: Option<Vec<u8>>,
    pub commitment: H256,
    pub l2_l1_merkle_root: H256,
    pub block_meta_params: L1BatchMetaParameters,
    pub aux_data_hash: H256,
    pub meta_parameters_hash: H256,
    pub pass_through_data_hash: H256,
    /// The commitment to the final events queue state after the batch is committed.
    /// Practically, it is a commitment to all events that happened on L2 during the batch execution.
    pub events_queue_commitment: Option<H256>,
    /// The commitment to the initial heap content of the bootloader. Practically it serves as a
    /// commitment to the transactions in the batch.
    pub bootloader_initial_content_commitment: Option<H256>,
    pub state_diffs_compressed: Vec<u8>,
}

impl L1BatchMetadata {
    pub fn tree_data(&self) -> L1BatchTreeData {
        L1BatchTreeData {
            hash: self.root_hash,
            rollup_last_leaf_index: self.rollup_last_leaf_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchWithMetadata {
    pub header: L1BatchHeader,
    pub metadata: L1BatchMetadata,
    pub raw_published_factory_deps: Vec<Vec<u8>>,
}

impl L1BatchWithMetadata {
    pub fn new(
        header: L1BatchHeader,
        metadata: L1BatchMetadata,
        unsorted_factory_deps: HashMap<H256, Vec<u8>>,
        raw_published_bytecode_hashes: &[H256],
    ) -> Self {
        Self {
            raw_published_factory_deps: Self::factory_deps_in_appearance_order(
                &header,
                unsorted_factory_deps,
                raw_published_bytecode_hashes,
            ),
            header,
            metadata,
        }
    }

    /// Iterates over factory deps in the order in which they appeared in this L1 batch.
    fn factory_deps_in_appearance_order(
        header: &L1BatchHeader,
        mut unsorted_factory_deps: HashMap<H256, Vec<u8>>,
        raw_published_bytecode_hashes: &[H256],
    ) -> Vec<Vec<u8>> {
        // TODO(PLA-731): ensure that the protocol version is always available.
        let protocol_version = header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        if protocol_version.is_pre_boojum() {
            header.l2_to_l1_logs.iter().filter_map(move |log| {
                let inner = &log.0;
                if inner.sender == KNOWN_CODES_STORAGE_ADDRESS {
                    let bytecode = unsorted_factory_deps.remove(&inner.key).unwrap_or_else(|| {
                        panic!(
                            "Failed to get bytecode that was marked as known: bytecode_hash {:?}, L1 batch number {:?}",
                            inner.key, header.number
                        );
                    });
                    Some(bytecode)
                } else {
                    None
                }
            }).collect()
        } else {
            raw_published_bytecode_hashes
                .iter()
                .map(|bytecode_hash| {
                    unsorted_factory_deps
                        .remove(bytecode_hash)
                        .unwrap_or_else(|| {
                            panic!(
                                "Failed to get bytecode that was marked as known: bytecode_hash {:?}, L1 batch number {:?}",
                                bytecode_hash, header.number
                            );
                        })
                })
                .collect()
        }
    }

    /// Packs all pubdata needed for batch commitment in boojum into one bytes array. The packing contains the
    /// following: logs, messages, bytecodes, and compressed state diffs.
    /// This data is currently part of calldata but will be submitted as part of the blob section post EIP-4844.
    pub fn construct_pubdata(&self) -> Vec<u8> {
        let mut res: Vec<u8> = vec![];

        // Process and Pack Logs
        res.extend((self.header.l2_to_l1_logs.len() as u32).to_be_bytes());
        for l2_to_l1_log in &self.header.l2_to_l1_logs {
            res.extend(l2_to_l1_log.0.to_bytes());
        }

        // Process and Pack Messages
        res.extend((self.header.l2_to_l1_messages.len() as u32).to_be_bytes());
        for msg in &self.header.l2_to_l1_messages {
            res.extend((msg.len() as u32).to_be_bytes());
            res.extend(msg);
        }

        // Process and Pack Bytecodes
        res.extend((self.raw_published_factory_deps.len() as u32).to_be_bytes());
        for bytecode in &self.raw_published_factory_deps {
            res.extend((bytecode.len() as u32).to_be_bytes());
            res.extend(bytecode);
        }

        // Extend with Compressed StateDiffs
        res.extend(&self.metadata.state_diffs_compressed);

        res
    }
}

impl SerializeCommitment for L2ToL1Log {
    const SERIALIZED_SIZE: usize = 88;

    fn serialize_commitment(&self, buffer: &mut [u8]) {
        buffer[0] = self.shard_id;
        buffer[1] = self.is_service as u8;
        buffer[2..4].copy_from_slice(&self.tx_number_in_block.to_be_bytes());
        buffer[4..24].copy_from_slice(self.sender.as_bytes());
        buffer[24..56].copy_from_slice(self.key.as_bytes());
        buffer[56..88].copy_from_slice(self.value.as_bytes());
    }
}

impl SerializeCommitment for UserL2ToL1Log {
    const SERIALIZED_SIZE: usize = L2ToL1Log::SERIALIZED_SIZE;

    fn serialize_commitment(&self, buffer: &mut [u8]) {
        self.0.serialize_commitment(buffer);
    }
}

impl SerializeCommitment for SystemL2ToL1Log {
    const SERIALIZED_SIZE: usize = L2ToL1Log::SERIALIZED_SIZE;

    fn serialize_commitment(&self, buffer: &mut [u8]) {
        self.0.serialize_commitment(buffer);
    }
}

impl SerializeCommitment for InitialStorageWrite {
    const SERIALIZED_SIZE: usize = 64;

    fn serialize_commitment(&self, buffer: &mut [u8]) {
        self.key.to_little_endian(&mut buffer[0..32]);
        buffer[32..].copy_from_slice(self.value.as_bytes());
    }
}

impl SerializeCommitment for RepeatedStorageWrite {
    const SERIALIZED_SIZE: usize = 40;

    fn serialize_commitment(&self, buffer: &mut [u8]) {
        buffer[..8].copy_from_slice(&self.index.to_be_bytes());
        buffer[8..].copy_from_slice(self.value.as_bytes());
    }
}

impl SerializeCommitment for StateDiffRecord {
    const SERIALIZED_SIZE: usize = PADDED_ENCODED_STORAGE_DIFF_LEN_BYTES;

    fn serialize_commitment(&self, buffer: &mut [u8]) {
        buffer.copy_from_slice(&self.encode_padded());
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
struct L1BatchAuxiliaryCommonOutput {
    l2_l1_logs_merkle_root: H256,
    protocol_version: ProtocolVersionId,
}

/// Block Output produced by Virtual Machine
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
enum L1BatchAuxiliaryOutput {
    PreBoojum {
        common: L1BatchAuxiliaryCommonOutput,
        l2_l1_logs_linear_hash: H256,
        initial_writes_compressed: Vec<u8>,
        initial_writes_hash: H256,
        repeated_writes_compressed: Vec<u8>,
        repeated_writes_hash: H256,
    },
    PostBoojum {
        common: L1BatchAuxiliaryCommonOutput,
        system_logs_linear_hash: H256,
        state_diffs_compressed: Vec<u8>,
        state_diffs_hash: H256,
        aux_commitments: AuxCommitments,
        blob_linear_hashes: Vec<H256>,
        blob_commitments: Vec<H256>,
    },
}

impl L1BatchAuxiliaryOutput {
    fn new(input: CommitmentInput) -> Self {
        match input {
            CommitmentInput::PreBoojum {
                common: common_input,
                initial_writes,
                repeated_writes,
            } => {
                let l2_l1_logs_compressed =
                    pre_boojum_serialize_commitments(&common_input.l2_to_l1_logs);
                // Skip first 4 bytes of the serialized logs (i.e., the number of logs).
                let merkle_tree_leaves = l2_l1_logs_compressed[4..]
                    .chunks(UserL2ToL1Log::SERIALIZED_SIZE)
                    .map(|chunk| <[u8; UserL2ToL1Log::SERIALIZED_SIZE]>::try_from(chunk).unwrap());
                let l2_l1_logs_merkle_root = MiniMerkleTree::new(
                    merkle_tree_leaves,
                    Some(l2_to_l1_logs_tree_size(common_input.protocol_version)),
                )
                .merkle_root();
                let l2_l1_logs_linear_hash = H256::from(keccak256(&l2_l1_logs_compressed));

                let common_output = L1BatchAuxiliaryCommonOutput {
                    l2_l1_logs_merkle_root,
                    protocol_version: common_input.protocol_version,
                };

                let initial_writes_compressed = pre_boojum_serialize_commitments(&initial_writes);
                let initial_writes_hash = H256::from(keccak256(&initial_writes_compressed));
                let repeated_writes_compressed = pre_boojum_serialize_commitments(&repeated_writes);
                let repeated_writes_hash = H256::from(keccak256(&repeated_writes_compressed));

                Self::PreBoojum {
                    common: common_output,
                    l2_l1_logs_linear_hash,
                    initial_writes_compressed,
                    initial_writes_hash,
                    repeated_writes_compressed,
                    repeated_writes_hash,
                }
            }
            CommitmentInput::PostBoojum {
                common: common_input,
                system_logs,
                state_diffs,
                aux_commitments,
                blob_commitments,
            } => {
                let l2_l1_logs_compressed = serialize_commitments(&common_input.l2_to_l1_logs);
                let merkle_tree_leaves = l2_l1_logs_compressed
                    .chunks(UserL2ToL1Log::SERIALIZED_SIZE)
                    .map(|chunk| <[u8; UserL2ToL1Log::SERIALIZED_SIZE]>::try_from(chunk).unwrap());
                let l2_l1_logs_merkle_root = MiniMerkleTree::new(
                    merkle_tree_leaves,
                    Some(l2_to_l1_logs_tree_size(common_input.protocol_version)),
                )
                .merkle_root();

                let common_output = L1BatchAuxiliaryCommonOutput {
                    l2_l1_logs_merkle_root,
                    protocol_version: common_input.protocol_version,
                };

                let system_logs_compressed = serialize_commitments(&system_logs);
                let system_logs_linear_hash = H256::from(keccak256(&system_logs_compressed));

                let state_diffs_packed = serialize_commitments(&state_diffs);
                let state_diffs_hash = H256::from(keccak256(&(state_diffs_packed)));
                let state_diffs_compressed = compress_state_diffs(state_diffs);

                let blob_linear_hashes =
                    parse_system_logs_for_blob_hashes(&common_input.protocol_version, &system_logs);

                // Sanity checks. System logs are empty for the genesis batch, so we can't do checks for it.
                if !system_logs.is_empty() {
                    let state_diff_hash_from_logs = system_logs
                        .iter()
                        .find_map(|log| {
                            (log.0.key == u256_to_h256(STATE_DIFF_HASH_KEY.into()))
                                .then_some(log.0.value)
                        })
                        .expect("Failed to find state diff hash in system logs");
                    assert_eq!(
                        state_diffs_hash, state_diff_hash_from_logs,
                        "State diff hash mismatch"
                    );

                    let l2_to_l1_logs_tree_root_from_logs = system_logs
                        .iter()
                        .find_map(|log| {
                            (log.0.key == u256_to_h256(L2_TO_L1_LOGS_TREE_ROOT_KEY.into()))
                                .then_some(log.0.value)
                        })
                        .expect("Failed to find L2 to L1 logs tree root in system logs");
                    assert_eq!(
                        l2_l1_logs_merkle_root, l2_to_l1_logs_tree_root_from_logs,
                        "L2 L1 logs tree root mismatch"
                    );
                }

                assert_eq!(
                    blob_linear_hashes.len(),
                    blob_commitments.len(),
                    "Blob linear hashes and commitments have different lengths"
                );

                Self::PostBoojum {
                    common: common_output,
                    system_logs_linear_hash,
                    state_diffs_compressed,
                    state_diffs_hash,
                    aux_commitments,
                    blob_linear_hashes,
                    blob_commitments,
                }
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();

        match self {
            Self::PreBoojum {
                common,
                l2_l1_logs_linear_hash,
                initial_writes_hash,
                repeated_writes_hash,
                ..
            } => {
                result.extend(common.l2_l1_logs_merkle_root.as_bytes());
                result.extend(l2_l1_logs_linear_hash.as_bytes());
                result.extend(initial_writes_hash.as_bytes());
                result.extend(repeated_writes_hash.as_bytes());
            }
            Self::PostBoojum {
                system_logs_linear_hash,
                state_diffs_hash,
                aux_commitments,
                blob_linear_hashes,
                blob_commitments,
                ..
            } => {
                result.extend(system_logs_linear_hash.as_bytes());
                result.extend(state_diffs_hash.as_bytes());
                result.extend(
                    aux_commitments
                        .bootloader_initial_content_commitment
                        .as_bytes(),
                );
                result.extend(aux_commitments.events_queue_commitment.as_bytes());

                for i in 0..blob_commitments.len() {
                    result.extend(blob_linear_hashes[i].as_bytes());
                    result.extend(blob_commitments[i].as_bytes());
                }
            }
        }

        result
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&keccak256(&self.to_bytes()))
    }

    pub fn common(&self) -> &L1BatchAuxiliaryCommonOutput {
        match self {
            Self::PreBoojum { common, .. } => common,
            Self::PostBoojum { common, .. } => common,
        }
    }
}

/// Meta parameters for an L1 batch. They are the same for each L1 batch per run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchMetaParameters {
    pub zkporter_is_available: bool,
    pub bootloader_code_hash: H256,
    pub default_aa_code_hash: H256,
    pub protocol_version: ProtocolVersionId,
}

impl L1BatchMetaParameters {
    pub fn to_bytes(&self) -> Vec<u8> {
        const SERIALIZED_SIZE: usize = 1 + 32 + 32 + 32;
        let mut result = Vec::with_capacity(SERIALIZED_SIZE);
        result.push(self.zkporter_is_available as u8);
        result.extend(self.bootloader_code_hash.as_bytes());
        result.extend(self.default_aa_code_hash.as_bytes());

        if self.protocol_version.is_post_1_5_0() {
            // EVM simulator hash for now is the same as the default AA hash.
            result.extend(self.default_aa_code_hash.as_bytes());
        }

        result
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&keccak256(&self.to_bytes()))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
struct RootState {
    pub last_leaf_index: u64,
    pub root_hash: H256,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
struct L1BatchPassThroughData {
    shared_states: Vec<RootState>,
}

impl L1BatchPassThroughData {
    pub fn to_bytes(&self) -> Vec<u8> {
        // We assume that currently we have only two shared state: Rollup and ZkPorter where porter is always zero
        const SERIALIZED_SIZE: usize = 8 + 32 + 8 + 32;
        let mut result = Vec::with_capacity(SERIALIZED_SIZE);
        for state in self.shared_states.iter() {
            result.extend_from_slice(&state.last_leaf_index.to_be_bytes());
            result.extend_from_slice(state.root_hash.as_bytes());
        }
        assert_eq!(
            result.len(),
            SERIALIZED_SIZE,
            "Serialized size for BlockPassThroughData is bigger than expected"
        );
        result
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&keccak256(&self.to_bytes()))
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchCommitment {
    pass_through_data: L1BatchPassThroughData,
    auxiliary_output: L1BatchAuxiliaryOutput,
    meta_parameters: L1BatchMetaParameters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
pub struct L1BatchCommitmentHash {
    pub pass_through_data: H256,
    pub aux_output: H256,
    pub meta_parameters: H256,
    pub commitment: H256,
}

impl L1BatchCommitment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(input: CommitmentInput) -> Self {
        let meta_parameters = L1BatchMetaParameters {
            zkporter_is_available: ZKPORTER_IS_AVAILABLE,
            bootloader_code_hash: input.common().bootloader_code_hash,
            default_aa_code_hash: input.common().default_aa_code_hash,
            protocol_version: input.common().protocol_version,
        };

        Self {
            pass_through_data: L1BatchPassThroughData {
                shared_states: vec![
                    RootState {
                        last_leaf_index: input.common().rollup_last_leaf_index,
                        root_hash: input.common().rollup_root_hash,
                    },
                    // Despite the fact that `zk_porter` is not available we have to add params about it.
                    RootState {
                        last_leaf_index: 0,
                        root_hash: H256::zero(),
                    },
                ],
            },
            auxiliary_output: L1BatchAuxiliaryOutput::new(input),
            meta_parameters,
        }
    }

    pub fn meta_parameters(&self) -> L1BatchMetaParameters {
        self.meta_parameters.clone()
    }

    pub fn l2_l1_logs_merkle_root(&self) -> H256 {
        self.auxiliary_output.common().l2_l1_logs_merkle_root
    }

    pub fn aux_commitments(&self) -> Option<AuxCommitments> {
        match &self.auxiliary_output {
            L1BatchAuxiliaryOutput::PostBoojum {
                aux_commitments, ..
            } => Some(*aux_commitments),
            L1BatchAuxiliaryOutput::PreBoojum { .. } => None,
        }
    }

    pub fn hash(&self) -> L1BatchCommitmentHash {
        let mut result = vec![];
        let pass_through_data_hash = self.pass_through_data.hash();
        result.extend_from_slice(pass_through_data_hash.as_bytes());
        let metadata_hash = self.meta_parameters.hash();
        result.extend_from_slice(metadata_hash.as_bytes());
        let auxiliary_output_hash = self.auxiliary_output.hash();
        result.extend_from_slice(auxiliary_output_hash.as_bytes());
        let hash = keccak256(&result);
        let commitment = H256::from_slice(&hash);
        L1BatchCommitmentHash {
            pass_through_data: pass_through_data_hash,
            aux_output: auxiliary_output_hash,
            meta_parameters: metadata_hash,
            commitment,
        }
    }

    pub fn artifacts(&self) -> L1BatchCommitmentArtifacts {
        let (compressed_initial_writes, compressed_repeated_writes, compressed_state_diffs) =
            match &self.auxiliary_output {
                L1BatchAuxiliaryOutput::PostBoojum {
                    state_diffs_compressed,
                    ..
                } => (None, None, Some(state_diffs_compressed.clone())),
                L1BatchAuxiliaryOutput::PreBoojum {
                    initial_writes_compressed,
                    repeated_writes_compressed,
                    ..
                } => (
                    Some(initial_writes_compressed.clone()),
                    Some(repeated_writes_compressed.clone()),
                    None,
                ),
            };

        L1BatchCommitmentArtifacts {
            commitment_hash: self.hash(),
            l2_l1_merkle_root: self.l2_l1_logs_merkle_root(),
            compressed_state_diffs,
            zkporter_is_available: self.meta_parameters.zkporter_is_available,
            aux_commitments: self.aux_commitments(),
            compressed_initial_writes,
            compressed_repeated_writes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
pub struct AuxCommitments {
    pub events_queue_commitment: H256,
    pub bootloader_initial_content_commitment: H256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
pub struct CommitmentCommonInput {
    pub l2_to_l1_logs: Vec<UserL2ToL1Log>,
    pub rollup_last_leaf_index: u64,
    pub rollup_root_hash: H256,
    pub bootloader_code_hash: H256,
    pub default_aa_code_hash: H256,
    pub protocol_version: ProtocolVersionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize, Deserialize))]
pub enum CommitmentInput {
    PreBoojum {
        common: CommitmentCommonInput,
        initial_writes: Vec<InitialStorageWrite>,
        repeated_writes: Vec<RepeatedStorageWrite>,
    },
    PostBoojum {
        common: CommitmentCommonInput,
        system_logs: Vec<SystemL2ToL1Log>,
        state_diffs: Vec<StateDiffRecord>,
        aux_commitments: AuxCommitments,
        blob_commitments: Vec<H256>,
    },
}

impl CommitmentInput {
    pub fn common(&self) -> &CommitmentCommonInput {
        match self {
            Self::PreBoojum { common, .. } => common,
            Self::PostBoojum { common, .. } => common,
        }
    }

    pub fn for_genesis_batch(
        rollup_root_hash: H256,
        rollup_last_leaf_index: u64,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        let commitment_common_input = CommitmentCommonInput {
            l2_to_l1_logs: Vec::new(),
            rollup_last_leaf_index,
            rollup_root_hash,
            bootloader_code_hash: base_system_contracts_hashes.bootloader,
            default_aa_code_hash: base_system_contracts_hashes.default_aa,
            protocol_version,
        };
        if protocol_version.is_pre_boojum() {
            Self::PreBoojum {
                common: commitment_common_input,
                initial_writes: Vec::new(),
                repeated_writes: Vec::new(),
            }
        } else {
            Self::PostBoojum {
                common: commitment_common_input,
                system_logs: Vec::new(),
                state_diffs: Vec::new(),
                aux_commitments: AuxCommitments {
                    events_queue_commitment: H256::zero(),
                    bootloader_initial_content_commitment: H256::zero(),
                },
                blob_commitments: {
                    let num_blobs = num_blobs_required(&protocol_version);

                    vec![H256::zero(); num_blobs]
                },
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L1BatchCommitmentArtifacts {
    pub commitment_hash: L1BatchCommitmentHash,
    pub l2_l1_merkle_root: H256,
    pub compressed_state_diffs: Option<Vec<u8>>,
    pub compressed_initial_writes: Option<Vec<u8>>,
    pub compressed_repeated_writes: Option<Vec<u8>>,
    pub zkporter_is_available: bool,
    pub aux_commitments: Option<AuxCommitments>,
}
