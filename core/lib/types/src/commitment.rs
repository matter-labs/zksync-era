//! Data structures that have more metadata than their primary versions declared in this crate.
//! For example, L1 batch defined here has the `root_hash` field which is absent in `L1BatchHeader`.
//!
//! Existence of this module is caused by the execution model of zkSync: when executing transactions,
//! we aim to avoid expensive operations like the state root hash recalculation. State root hash is not
//! required for the rollup to execute L1 batches, it's needed for the proof generation and the Ethereum
//! transactions, thus the calculations are done separately and asynchronously.

use serde::{Deserialize, Serialize};
use zksync_utils::u256_to_h256;

use std::{collections::HashMap, convert::TryFrom};

use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_system_constants::{
    L2_TO_L1_LOGS_TREE_ROOT_KEY, STATE_DIFF_HASH_KEY, ZKPORTER_IS_AVAILABLE,
};

use crate::{
    block::L1BatchHeader,
    ethabi::Token,
    l2_to_l1_log::{L2ToL1Log, SystemL2ToL1Log, UserL2ToL1Log},
    web3::signing::keccak256,
    writes::{
        compress_state_diffs, InitialStorageWrite, RepeatedStorageWrite, StateDiffRecord,
        PADDED_ENCODED_STORAGE_DIFF_LEN_BYTES,
    },
    H256, KNOWN_CODES_STORAGE_ADDRESS, U256,
};

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
pub(crate) fn pre_boojum_serialize_commitments<I: SerializeCommitment>(values: &[I]) -> Vec<u8> {
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
    pub merkle_root_hash: H256,
    pub initial_writes_compressed: Vec<u8>,
    pub repeated_writes_compressed: Vec<u8>,
    pub commitment: H256,
    pub l2_l1_messages_compressed: Vec<u8>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchWithMetadata {
    pub header: L1BatchHeader,
    pub metadata: L1BatchMetadata,
    pub factory_deps: Vec<Vec<u8>>,
}

impl L1BatchWithMetadata {
    pub fn new(
        header: L1BatchHeader,
        metadata: L1BatchMetadata,
        unsorted_factory_deps: HashMap<H256, Vec<u8>>,
    ) -> Self {
        Self {
            factory_deps: Self::factory_deps_in_appearance_order(&header, &unsorted_factory_deps)
                .map(<[u8]>::to_vec)
                .collect(),
            header,
            metadata,
        }
    }

    /// Iterates over factory deps in the order in which they appeared in this L1 batch.
    pub fn factory_deps_in_appearance_order<'a>(
        header: &'a L1BatchHeader,
        unsorted_factory_deps: &'a HashMap<H256, Vec<u8>>,
    ) -> impl Iterator<Item = &'a [u8]> + 'a {
        header.l2_to_l1_logs.iter().filter_map(move |log| {
            let inner = &log.0;
            if inner.sender == KNOWN_CODES_STORAGE_ADDRESS {
                let bytecode = unsorted_factory_deps.get(&inner.key).unwrap_or_else(|| {
                    panic!(
                        "Failed to get bytecode that was marked as known: bytecode_hash {:?}, \
                             L1 batch number {:?}",
                        inner.key, header.number
                    );
                });
                Some(bytecode.as_slice())
            } else {
                None
            }
        })
    }

    pub fn l1_header_data(&self) -> Token {
        Token::Tuple(vec![
            Token::Uint(U256::from(self.header.number.0)),
            Token::FixedBytes(self.metadata.root_hash.as_bytes().to_vec()),
            Token::Uint(U256::from(self.metadata.rollup_last_leaf_index)),
            Token::Uint(U256::from(self.header.l1_tx_count)),
            Token::FixedBytes(
                self.header
                    .priority_ops_onchain_data_hash()
                    .as_bytes()
                    .to_vec(),
            ),
            Token::FixedBytes(self.metadata.l2_l1_merkle_root.as_bytes().to_vec()),
            Token::Uint(U256::from(self.header.timestamp)),
            Token::FixedBytes(self.metadata.commitment.as_bytes().to_vec()),
        ])
    }

    pub fn l1_commit_data(&self) -> Token {
        if self.header.protocol_version.unwrap().is_pre_boojum() {
            Token::Tuple(vec![
                Token::Uint(U256::from(self.header.number.0)),
                Token::Uint(U256::from(self.header.timestamp)),
                Token::Uint(U256::from(self.metadata.rollup_last_leaf_index)),
                Token::FixedBytes(self.metadata.merkle_root_hash.as_bytes().to_vec()),
                Token::Uint(U256::from(self.header.l1_tx_count)),
                Token::FixedBytes(self.metadata.l2_l1_merkle_root.as_bytes().to_vec()),
                Token::FixedBytes(
                    self.header
                        .priority_ops_onchain_data_hash()
                        .as_bytes()
                        .to_vec(),
                ),
                Token::Bytes(self.metadata.initial_writes_compressed.clone()),
                Token::Bytes(self.metadata.repeated_writes_compressed.clone()),
                Token::Bytes(self.metadata.l2_l1_messages_compressed.clone()),
                Token::Array(
                    self.header
                        .l2_to_l1_messages
                        .iter()
                        .map(|message| Token::Bytes(message.to_vec()))
                        .collect(),
                ),
                Token::Array(
                    self.factory_deps
                        .iter()
                        .map(|bytecode| Token::Bytes(bytecode.to_vec()))
                        .collect(),
                ),
            ])
        } else {
            Token::Tuple(vec![
                Token::Uint(U256::from(self.header.number.0)),
                Token::Uint(U256::from(self.header.timestamp)),
                Token::Uint(U256::from(self.metadata.rollup_last_leaf_index)),
                Token::FixedBytes(self.metadata.merkle_root_hash.as_bytes().to_vec()),
                Token::Uint(U256::from(self.header.l1_tx_count)),
                Token::FixedBytes(
                    self.header
                        .priority_ops_onchain_data_hash()
                        .as_bytes()
                        .to_vec(),
                ),
                Token::FixedBytes(
                    self.metadata
                        .bootloader_initial_content_commitment
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
                Token::FixedBytes(
                    self.metadata
                        .events_queue_commitment
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                ),
                Token::Bytes(self.metadata.l2_l1_messages_compressed.clone()),
                Token::Bytes(self.construct_pubdata()),
            ])
        }
    }

    pub fn l1_commit_data_size(&self) -> usize {
        crate::ethabi::encode(&[Token::Array(vec![self.l1_commit_data()])]).len()
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

        // Process and Pack Msgs
        res.extend((self.header.l2_to_l1_messages.len() as u32).to_be_bytes());
        for msg in &self.header.l2_to_l1_messages {
            res.extend((msg.len() as u32).to_be_bytes());
            res.extend(msg);
        }

        // Process and Pack Bytecodes
        res.extend((self.factory_deps.len() as u32).to_be_bytes());
        for bytecode in &self.factory_deps {
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

/// Block Output produced by Virtual Machine
#[derive(Debug, Clone)]
struct L1BatchAuxiliaryOutput {
    // We use initial fields for debugging
    #[allow(dead_code)]
    l2_l1_logs: Vec<UserL2ToL1Log>,
    #[allow(dead_code)]
    initial_writes: Vec<InitialStorageWrite>,
    #[allow(dead_code)]
    repeated_writes: Vec<RepeatedStorageWrite>,

    l2_l1_logs_compressed: Vec<u8>,
    l2_l1_logs_linear_hash: H256,
    l2_l1_logs_merkle_root: H256,

    // Once cut over to boojum, these fields are no longer required as their values
    // are covered by state_diffs_compressed and its hash.
    // Task to remove: PLA-640
    initial_writes_compressed: Vec<u8>,
    initial_writes_hash: H256,
    repeated_writes_compressed: Vec<u8>,
    repeated_writes_hash: H256,

    // The fields below are necessary for boojum.
    system_logs_compressed: Vec<u8>,
    #[allow(dead_code)]
    system_logs_linear_hash: H256,
    #[allow(dead_code)]
    state_diffs_hash: H256,
    state_diffs_compressed: Vec<u8>,
    #[allow(dead_code)]
    bootloader_heap_hash: H256,
    #[allow(dead_code)]
    events_state_queue_hash: H256,
    is_pre_boojum: bool,
}

impl L1BatchAuxiliaryOutput {
    #[allow(clippy::too_many_arguments)]
    fn new(
        l2_l1_logs: Vec<UserL2ToL1Log>,
        initial_writes: Vec<InitialStorageWrite>,
        repeated_writes: Vec<RepeatedStorageWrite>,
        system_logs: Vec<SystemL2ToL1Log>,
        state_diffs: Vec<StateDiffRecord>,
        bootloader_heap_hash: H256,
        events_state_queue_hash: H256,
        is_pre_boojum: bool,
    ) -> Self {
        let state_diff_hash_from_logs = system_logs.iter().find_map(|log| {
            if log.0.key == u256_to_h256(STATE_DIFF_HASH_KEY.into()) {
                Some(log.0.value)
            } else {
                None
            }
        });

        let merke_tree_root_from_logs = system_logs.iter().find_map(|log| {
            if log.0.key == u256_to_h256(L2_TO_L1_LOGS_TREE_ROOT_KEY.into()) {
                Some(log.0.value)
            } else {
                None
            }
        });

        let (
            l2_l1_logs_compressed,
            initial_writes_compressed,
            repeated_writes_compressed,
            system_logs_compressed,
            state_diffs_packed,
        ) = if is_pre_boojum {
            (
                pre_boojum_serialize_commitments(&l2_l1_logs),
                pre_boojum_serialize_commitments(&initial_writes),
                pre_boojum_serialize_commitments(&repeated_writes),
                pre_boojum_serialize_commitments(&system_logs),
                pre_boojum_serialize_commitments(&state_diffs),
            )
        } else {
            (
                serialize_commitments(&l2_l1_logs),
                serialize_commitments(&initial_writes),
                serialize_commitments(&repeated_writes),
                serialize_commitments(&system_logs),
                serialize_commitments(&state_diffs),
            )
        };

        let state_diffs_compressed = compress_state_diffs(state_diffs.clone());

        let l2_l1_logs_linear_hash = H256::from(keccak256(&l2_l1_logs_compressed));
        let system_logs_linear_hash = H256::from(keccak256(&system_logs_compressed));
        let initial_writes_hash = H256::from(keccak256(&initial_writes_compressed));
        let repeated_writes_hash = H256::from(keccak256(&repeated_writes_compressed));
        let state_diffs_hash = H256::from(keccak256(&(state_diffs_packed)));

        let serialized_logs = if is_pre_boojum {
            &l2_l1_logs_compressed[4..]
        } else {
            &l2_l1_logs_compressed
        };

        let merkle_tree_leaves = serialized_logs
            .chunks(UserL2ToL1Log::SERIALIZED_SIZE)
            .map(|chunk| <[u8; UserL2ToL1Log::SERIALIZED_SIZE]>::try_from(chunk).unwrap());
        // ^ Skip first 4 bytes of the serialized logs (i.e., the number of logs).
        let min_tree_size = if is_pre_boojum {
            L2ToL1Log::PRE_BOOJUM_MIN_L2_L1_LOGS_TREE_SIZE
        } else {
            L2ToL1Log::MIN_L2_L1_LOGS_TREE_SIZE
        };
        let l2_l1_logs_merkle_root =
            MiniMerkleTree::new(merkle_tree_leaves, Some(min_tree_size)).merkle_root();

        if !system_logs.is_empty() {
            assert_eq!(
                state_diffs_hash,
                state_diff_hash_from_logs.unwrap(),
                "State diff hash mismatch"
            );
            assert_eq!(
                l2_l1_logs_merkle_root,
                merke_tree_root_from_logs.unwrap(),
                "L2 L1 logs tree root mismatch"
            );
        }

        Self {
            l2_l1_logs_compressed,
            initial_writes_compressed,
            repeated_writes_compressed,
            l2_l1_logs,
            initial_writes,
            repeated_writes,
            l2_l1_logs_linear_hash,
            l2_l1_logs_merkle_root,
            initial_writes_hash,
            repeated_writes_hash,
            system_logs_compressed,
            system_logs_linear_hash,
            state_diffs_hash,
            state_diffs_compressed,

            bootloader_heap_hash,
            events_state_queue_hash,
            is_pre_boojum,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        // 4 H256 values
        const SERIALIZED_SIZE: usize = 128;
        let mut result = Vec::with_capacity(SERIALIZED_SIZE);

        if self.is_pre_boojum {
            result.extend(self.l2_l1_logs_merkle_root.as_bytes());
            result.extend(self.l2_l1_logs_linear_hash.as_bytes());
            result.extend(self.initial_writes_hash.as_bytes());
            result.extend(self.repeated_writes_hash.as_bytes());
        } else {
            result.extend(self.system_logs_linear_hash.as_bytes());
            result.extend(self.state_diffs_hash.as_bytes());
            result.extend(self.bootloader_heap_hash.as_bytes());
            result.extend(self.events_state_queue_hash.as_bytes());
        }
        result
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&keccak256(&self.to_bytes()))
    }
}

/// Meta parameters for an L1 batch. They are the same for each L1 batch per run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchMetaParameters {
    pub zkporter_is_available: bool,
    pub bootloader_code_hash: H256,
    pub default_aa_code_hash: H256,
}

impl L1BatchMetaParameters {
    pub fn to_bytes(&self) -> Vec<u8> {
        const SERIALIZED_SIZE: usize = 4 + 1 + 32 + 32;
        let mut result = Vec::with_capacity(SERIALIZED_SIZE);
        result.push(self.zkporter_is_available as u8);
        result.extend(self.bootloader_code_hash.as_bytes());
        result.extend(self.default_aa_code_hash.as_bytes());
        result
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&keccak256(&self.to_bytes()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RootState {
    pub last_leaf_index: u64,
    pub root_hash: H256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
pub struct L1BatchCommitmentHash {
    pub pass_through_data: H256,
    pub aux_output: H256,
    pub meta_parameters: H256,
    pub commitment: H256,
}

impl L1BatchCommitment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        l2_to_l1_logs: Vec<UserL2ToL1Log>,
        rollup_last_leaf_index: u64,
        rollup_root_hash: H256,
        initial_writes: Vec<InitialStorageWrite>,
        repeated_writes: Vec<RepeatedStorageWrite>,
        bootloader_code_hash: H256,
        default_aa_code_hash: H256,
        system_logs: Vec<SystemL2ToL1Log>,
        state_diffs: Vec<StateDiffRecord>,
        bootloader_heap_hash: H256,
        events_state_queue_hash: H256,
        is_pre_boojum: bool,
    ) -> Self {
        let meta_parameters = L1BatchMetaParameters {
            zkporter_is_available: ZKPORTER_IS_AVAILABLE,
            bootloader_code_hash,
            default_aa_code_hash,
        };

        Self {
            pass_through_data: L1BatchPassThroughData {
                shared_states: vec![
                    RootState {
                        last_leaf_index: rollup_last_leaf_index,
                        root_hash: rollup_root_hash,
                    },
                    // Despite the fact that zk_porter is not available we have to add params about it.
                    RootState {
                        last_leaf_index: 0,
                        root_hash: H256::zero(),
                    },
                ],
            },
            auxiliary_output: L1BatchAuxiliaryOutput::new(
                l2_to_l1_logs,
                initial_writes,
                repeated_writes,
                system_logs,
                state_diffs,
                bootloader_heap_hash,
                events_state_queue_hash,
                is_pre_boojum,
            ),
            meta_parameters,
        }
    }

    pub fn meta_parameters(&self) -> L1BatchMetaParameters {
        self.meta_parameters.clone()
    }

    pub fn l2_l1_logs_compressed(&self) -> &[u8] {
        &self.auxiliary_output.l2_l1_logs_compressed
    }

    pub fn l2_l1_logs_linear_hash(&self) -> H256 {
        self.auxiliary_output.l2_l1_logs_linear_hash
    }

    pub fn l2_l1_logs_merkle_root(&self) -> H256 {
        self.auxiliary_output.l2_l1_logs_merkle_root
    }

    pub fn initial_writes_compressed(&self) -> &[u8] {
        &self.auxiliary_output.initial_writes_compressed
    }

    pub fn repeated_writes_compressed(&self) -> &[u8] {
        &self.auxiliary_output.repeated_writes_compressed
    }

    pub fn initial_writes_pubdata_hash(&self) -> H256 {
        self.auxiliary_output.initial_writes_hash
    }

    pub fn repeated_writes_pubdata_hash(&self) -> H256 {
        self.auxiliary_output.repeated_writes_hash
    }

    pub fn system_logs_compressed(&self) -> &[u8] {
        &self.auxiliary_output.system_logs_compressed
    }

    pub fn state_diffs_compressed(&self) -> &[u8] {
        &self.auxiliary_output.state_diffs_compressed
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
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_with::serde_as;

    use crate::commitment::{
        L1BatchAuxiliaryOutput, L1BatchCommitment, L1BatchMetaParameters, L1BatchPassThroughData,
    };
    use crate::l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log};
    use crate::writes::{InitialStorageWrite, RepeatedStorageWrite};
    use crate::{H256, U256};

    #[serde_as]
    #[derive(Debug, Serialize, Deserialize)]
    struct ExpectedOutput {
        #[serde_as(as = "serde_with::hex::Hex")]
        l2_l1_bytes: Vec<u8>,
        l2_l1_linear_hash: H256,
        l2_l1_root_hash: H256,
        #[serde_as(as = "serde_with::hex::Hex")]
        initial_writes_bytes: Vec<u8>,
        initial_writes_hash: H256,
        #[serde_as(as = "serde_with::hex::Hex")]
        repeated_writes_bytes: Vec<u8>,
        repeated_writes_hash: H256,
        #[serde_as(as = "serde_with::hex::Hex")]
        pass_through_bytes: Vec<u8>,
        pass_through_hash: H256,
        #[serde_as(as = "serde_with::hex::Hex")]
        meta_params_bytes: Vec<u8>,
        meta_params_hash: H256,
        #[serde_as(as = "serde_with::hex::Hex")]
        auxiliary_bytes: Vec<u8>,
        auxiliary_hash: H256,
        commitment_hash: H256,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct InitialStorageTest {
        pub key: String,
        pub value: H256,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct BlockAuxiliaryInput {
        l2_l1_logs: Vec<L2ToL1Log>,
        initial_writes: Vec<InitialStorageTest>,
        repeated_writes: Vec<RepeatedStorageWrite>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct CommitmentTest {
        pass_through_data: L1BatchPassThroughData,
        auxiliary_input: BlockAuxiliaryInput,
        meta_parameters: L1BatchMetaParameters,
        expected_outputs: ExpectedOutput,
    }

    // TODO(PLA-568): restore this test
    #[ignore]
    #[test]
    fn commitment_test() {
        let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&zksync_home)
            .join("etc/commitment_tests/zksync_testharness_test.json");
        let contents = std::fs::read_to_string(path).unwrap();
        let commitment_test: CommitmentTest = serde_json::from_str(&contents).unwrap();

        let initial_writes = commitment_test
            .auxiliary_input
            .initial_writes
            .clone()
            .into_iter()
            .enumerate()
            .map(|(index, a)| InitialStorageWrite {
                index: index as u64 + 1,
                key: U256::from_dec_str(&a.key).unwrap(),
                value: a.value,
            })
            .collect();
        let auxiliary_output = L1BatchAuxiliaryOutput::new(
            commitment_test
                .auxiliary_input
                .l2_l1_logs
                .into_iter()
                .map(UserL2ToL1Log)
                .collect(),
            initial_writes,
            commitment_test.auxiliary_input.repeated_writes.clone(),
            vec![],
            vec![],
            H256::zero(),
            H256::zero(),
            false,
        );

        let commitment = L1BatchCommitment {
            pass_through_data: commitment_test.pass_through_data,
            auxiliary_output,
            meta_parameters: commitment_test.meta_parameters,
        };

        assert_eq!(
            commitment.auxiliary_output.l2_l1_logs_compressed.len(),
            commitment_test.expected_outputs.l2_l1_bytes.len()
        );
        assert_eq!(
            commitment.auxiliary_output.l2_l1_logs_compressed,
            commitment_test.expected_outputs.l2_l1_bytes
        );
        assert_eq!(
            commitment.auxiliary_output.l2_l1_logs_linear_hash,
            commitment_test.expected_outputs.l2_l1_linear_hash
        );
        assert_eq!(
            commitment.auxiliary_output.l2_l1_logs_merkle_root,
            commitment_test.expected_outputs.l2_l1_root_hash
        );

        assert_eq!(
            commitment.auxiliary_output.repeated_writes_compressed.len(),
            commitment_test.expected_outputs.repeated_writes_bytes.len()
        );
        assert_eq!(
            commitment.auxiliary_output.repeated_writes_compressed,
            commitment_test.expected_outputs.repeated_writes_bytes
        );

        assert_eq!(
            commitment.auxiliary_output.repeated_writes_hash,
            commitment_test.expected_outputs.repeated_writes_hash
        );
        assert_eq!(
            commitment.auxiliary_output.initial_writes_compressed.len(),
            commitment_test.expected_outputs.initial_writes_bytes.len()
        );
        assert_eq!(
            commitment.auxiliary_output.initial_writes_compressed,
            commitment_test.expected_outputs.initial_writes_bytes
        );

        assert_eq!(
            commitment.auxiliary_output.initial_writes_hash,
            commitment_test.expected_outputs.initial_writes_hash
        );
        assert_eq!(
            commitment.pass_through_data.to_bytes(),
            commitment_test.expected_outputs.pass_through_bytes
        );
        assert_eq!(
            commitment.pass_through_data.hash(),
            commitment_test.expected_outputs.pass_through_hash
        );
        assert_eq!(
            commitment.meta_parameters.to_bytes(),
            commitment_test.expected_outputs.meta_params_bytes,
        );
        assert_eq!(
            commitment.meta_parameters.hash(),
            commitment_test.expected_outputs.meta_params_hash,
        );
        assert_eq!(
            commitment.auxiliary_output.to_bytes(),
            commitment_test.expected_outputs.auxiliary_bytes
        );
        assert_eq!(
            commitment.auxiliary_output.hash(),
            commitment_test.expected_outputs.auxiliary_hash
        );
        assert_eq!(
            commitment.hash().commitment,
            commitment_test.expected_outputs.commitment_hash
        );
    }
}
