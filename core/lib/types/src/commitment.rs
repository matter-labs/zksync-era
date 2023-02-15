//! Data structures that have more metadata than their primary versions declared in this crate.
//! For example, block defined here has the `root_hash` field which is absent in the usual `Block`.
//!
//! Existence of this module is caused by the execution model of zkSync: when executing transactions,
//! we aim to avoid expensive operations like the state root hash recalculation. State root hash is not
//! required for the rollup to execute blocks, it's needed for the proof generation and the Ethereum
//! transactions, thus the calculations are done separately and asynchronously.

use std::collections::HashMap;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zksync_contracts::{DEFAULT_ACCOUNT_CODE, PROVED_BLOCK_BOOTLOADER_CODE};

use zksync_config::constants::ZKPORTER_IS_AVAILABLE;
use zksync_mini_merkle_tree::mini_merkle_tree_root_hash;
use zksync_utils::u256_to_h256;

use crate::circuit::GEOMETRY_CONFIG;
use crate::ethabi::Token;
use crate::l2_to_l1_log::L2ToL1Log;
use crate::web3::signing::keccak256;
use crate::writes::{InitialStorageWrite, RepeatedStorageWrite};
use crate::{block::L1BatchHeader, H256, KNOWN_CODES_STORAGE_ADDRESS, U256};

/// Make the struct serializable for commitment.
pub trait CommitmentSerializable: Clone {
    /// Size of the structure in bytes
    const SERIALIZED_SIZE: usize;
    /// The number of objects of this type that can be included in the block
    fn limit_per_block() -> usize;
    fn to_bytes(self) -> Vec<u8>;
}

/// Serialize elements for commitment. The results consist of:
/// 1. Number of elements (4 bytes)
/// 2. Serialized elements
pub(crate) fn serialize_commitments<I: CommitmentSerializable>(values: &[I]) -> Vec<u8> {
    let final_len = I::limit_per_block() * I::SERIALIZED_SIZE + 4;
    let mut input = Vec::with_capacity(final_len);
    let result = values
        .iter()
        .cloned()
        .flat_map(CommitmentSerializable::to_bytes);
    input.extend((values.len() as u32).to_be_bytes());
    input.extend(result);
    assert!(
        input.len() <= final_len,
        "The size of serialized values is more than expected expected_len {} actual_len {} size {} capacity {}",
        final_len,  input.len() , I::SERIALIZED_SIZE,   I::limit_per_block()
    );
    input
}

/// Precalculated data for the block that was used in commitment and L1 transaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockMetadata {
    pub root_hash: H256,
    pub rollup_last_leaf_index: u64,
    pub merkle_root_hash: H256,
    pub initial_writes_compressed: Vec<u8>,
    pub repeated_writes_compressed: Vec<u8>,
    pub commitment: H256,
    pub l2_l1_messages_compressed: Vec<u8>,
    pub l2_l1_merkle_root: H256,
    pub block_meta_params: BlockMetaParameters,
    pub aux_data_hash: H256,
    pub meta_parameters_hash: H256,
    pub pass_through_data_hash: H256,
}

impl BlockMetadata {
    /// Mock metadata, exists only for tests.
    #[doc(hidden)]
    pub fn mock() -> Self {
        Self {
            root_hash: H256::zero(),
            rollup_last_leaf_index: 1,
            merkle_root_hash: H256::zero(),
            initial_writes_compressed: vec![],
            repeated_writes_compressed: vec![],
            commitment: Default::default(),
            l2_l1_messages_compressed: vec![],
            l2_l1_merkle_root: H256::default(),
            block_meta_params: BlockMetaParameters::default(),
            aux_data_hash: Default::default(),
            meta_parameters_hash: Default::default(),
            pass_through_data_hash: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockWithMetadata {
    pub header: L1BatchHeader,
    pub metadata: BlockMetadata,
    pub factory_deps: Vec<Vec<u8>>,
}

impl BlockWithMetadata {
    pub fn new(
        header: L1BatchHeader,
        metadata: BlockMetadata,
        unsorted_factory_deps: HashMap<H256, Vec<u8>>,
    ) -> Self {
        Self {
            factory_deps: Self::factory_deps_in_appearance_order(&header, &unsorted_factory_deps),
            header,
            metadata,
        }
    }

    /// Creates an array of factory deps in the order in which they appeared in a block
    fn factory_deps_in_appearance_order(
        header: &L1BatchHeader,
        unsorted_factory_deps: &HashMap<H256, Vec<u8>>,
    ) -> Vec<Vec<u8>> {
        let mut result = Vec::with_capacity(unsorted_factory_deps.len());

        for log in &header.l2_to_l1_logs {
            if log.sender == KNOWN_CODES_STORAGE_ADDRESS {
                result.push(
                    unsorted_factory_deps
                        .get(&log.key)
                        .unwrap_or_else(|| panic!("Failed to get bytecode that was marked as known on L2 block: bytecodehash: {:?}, block number {:?}", &log.key, header.number))
                        .clone(),
                );
            }
        }

        result
    }

    pub fn l1_header_data(&self) -> Token {
        Token::Tuple(vec![
            Token::Uint(U256::from(*self.header.number)),
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
    }

    pub fn l1_commit_data_size(&self) -> usize {
        crate::ethabi::encode(&[Token::Array(vec![self.l1_commit_data()])]).len()
    }
}

impl CommitmentSerializable for L2ToL1Log {
    const SERIALIZED_SIZE: usize = 88;

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_l1_messages_merklizer as usize
    }

    fn to_bytes(self) -> Vec<u8> {
        let mut raw_data = Vec::with_capacity(Self::SERIALIZED_SIZE);
        raw_data.push(self.shard_id);
        raw_data.push(self.is_service as u8);
        raw_data.extend(self.tx_number_in_block.to_be_bytes());
        raw_data.extend(self.sender.as_bytes());
        raw_data.extend(self.key.as_bytes());
        raw_data.extend(self.value.as_bytes());
        assert_eq!(
            raw_data.len(),
            Self::SERIALIZED_SIZE,
            "Serialized size for L2ToL1Log is bigger than expected"
        );
        raw_data
    }
}

impl CommitmentSerializable for InitialStorageWrite {
    const SERIALIZED_SIZE: usize = 64;

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_initial_writes_pubdata_hasher as usize
    }

    fn to_bytes(self) -> Vec<u8> {
        let mut result = vec![0; Self::SERIALIZED_SIZE];
        self.key.to_little_endian(&mut result[0..32]);
        result[32..64].copy_from_slice(self.value.as_bytes());
        result
    }
}

impl CommitmentSerializable for RepeatedStorageWrite {
    const SERIALIZED_SIZE: usize = 40;

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_repeated_writes_pubdata_hasher as usize
    }

    fn to_bytes(self) -> Vec<u8> {
        let mut result = Vec::with_capacity(Self::SERIALIZED_SIZE);
        result.extend_from_slice(&self.index.to_be_bytes());
        result.extend_from_slice(self.value.as_bytes());
        assert_eq!(
            result.len(),
            Self::SERIALIZED_SIZE,
            "Serialized size for RepeatedStorageWrite is bigger than expected"
        );
        result
    }
}

/// Block Output produced by Virtual Machine
#[derive(Debug, Clone)]
struct BlockAuxiliaryOutput {
    // We use initial fields for debugging
    #[allow(dead_code)]
    l2_l1_logs: Vec<L2ToL1Log>,
    #[allow(dead_code)]
    initial_writes: Vec<InitialStorageWrite>,
    #[allow(dead_code)]
    repeated_writes: Vec<RepeatedStorageWrite>,
    l2_l1_logs_compressed: Vec<u8>,
    l2_l1_logs_linear_hash: H256,
    l2_l1_logs_merkle_root: H256,
    initial_writes_compressed: Vec<u8>,
    initial_writes_hash: H256,
    repeated_writes_compressed: Vec<u8>,
    repeated_writes_hash: H256,
}

impl BlockAuxiliaryOutput {
    fn new(
        l2_l1_logs: Vec<L2ToL1Log>,
        initial_writes: Vec<InitialStorageWrite>,
        repeated_writes: Vec<RepeatedStorageWrite>,
    ) -> Self {
        let l2_l1_logs_compressed = serialize_commitments(&l2_l1_logs);
        let initial_writes_compressed = serialize_commitments(&initial_writes);
        let repeated_writes_compressed = serialize_commitments(&repeated_writes);

        let l2_l1_logs_linear_hash = H256::from(keccak256(&l2_l1_logs_compressed));
        let initial_writes_hash = H256::from(keccak256(&initial_writes_compressed));
        let repeated_writes_hash = H256::from(keccak256(&repeated_writes_compressed));

        let l2_l1_logs_merkle_root = {
            let values: Vec<Vec<u8>> = l2_l1_logs
                .iter()
                .cloned()
                .map(CommitmentSerializable::to_bytes)
                .collect();
            mini_merkle_tree_root_hash(
                values,
                L2ToL1Log::SERIALIZED_SIZE,
                L2ToL1Log::limit_per_block(),
            )
        };

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
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        // 4 H256 values
        const SERIALIZED_SIZE: usize = 128;
        let mut result = Vec::with_capacity(SERIALIZED_SIZE);
        result.extend(self.l2_l1_logs_merkle_root.as_bytes());
        result.extend(self.l2_l1_logs_linear_hash.as_bytes());
        result.extend(self.initial_writes_hash.as_bytes());
        result.extend(self.repeated_writes_hash.as_bytes());
        result
    }

    pub fn hash(&self) -> H256 {
        H256::from_slice(&keccak256(&self.to_bytes()))
    }
}

/// Meta parameters for block. They are the same for each block per run, excluding timestamp.
/// We keep timestamp in seconds here for consistency with the crypto team
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockMetaParameters {
    pub zkporter_is_available: bool,
    pub bootloader_code_hash: H256,
    pub default_aa_code_hash: H256,
}

impl Default for BlockMetaParameters {
    fn default() -> Self {
        Self {
            zkporter_is_available: ZKPORTER_IS_AVAILABLE,
            bootloader_code_hash: u256_to_h256(PROVED_BLOCK_BOOTLOADER_CODE.hash),
            default_aa_code_hash: u256_to_h256(DEFAULT_ACCOUNT_CODE.hash),
        }
    }
}

impl BlockMetaParameters {
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
struct BlockPassThroughData {
    shared_states: Vec<RootState>,
}

impl BlockPassThroughData {
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
pub struct BlockCommitment {
    pass_through_data: BlockPassThroughData,
    auxiliary_output: BlockAuxiliaryOutput,
    meta_parameters: BlockMetaParameters,
}

#[derive(Debug, Clone)]
pub struct BlockCommitmentHash {
    pub pass_through_data: H256,
    pub aux_output: H256,
    pub meta_parameters: H256,
    pub commitment: H256,
}

impl BlockCommitment {
    pub fn new(
        l2_to_l1_logs: Vec<L2ToL1Log>,
        rollup_last_leaf_index: u64,
        rollup_root_hash: H256,
        initial_writes: Vec<InitialStorageWrite>,
        repeated_writes: Vec<RepeatedStorageWrite>,
    ) -> Self {
        let meta_parameters = BlockMetaParameters::default();

        Self {
            pass_through_data: BlockPassThroughData {
                shared_states: vec![
                    RootState {
                        last_leaf_index: rollup_last_leaf_index,
                        root_hash: rollup_root_hash,
                    },
                    // Despite the fact, that zk_porter is not available we have to add params about it.
                    RootState {
                        last_leaf_index: 0,
                        root_hash: H256::zero(),
                    },
                ],
            },
            auxiliary_output: BlockAuxiliaryOutput::new(
                l2_to_l1_logs,
                initial_writes,
                repeated_writes,
            ),
            meta_parameters,
        }
    }

    pub fn meta_parameters(&self) -> BlockMetaParameters {
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

    pub fn hash(&self) -> BlockCommitmentHash {
        let mut result = vec![];
        let pass_through_data_hash = self.pass_through_data.hash();
        result.extend_from_slice(pass_through_data_hash.as_bytes());
        let metadata_hash = self.meta_parameters.hash();
        result.extend_from_slice(metadata_hash.as_bytes());
        let auxiliary_output_hash = self.auxiliary_output.hash();
        result.extend_from_slice(auxiliary_output_hash.as_bytes());
        let hash = keccak256(&result);
        let commitment = H256::from_slice(&hash);
        BlockCommitmentHash {
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
        BlockAuxiliaryOutput, BlockCommitment, BlockMetaParameters, BlockPassThroughData,
    };
    use crate::l2_to_l1_log::L2ToL1Log;
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
        pass_through_data: BlockPassThroughData,
        auxiliary_input: BlockAuxiliaryInput,
        meta_parameters: BlockMetaParameters,
        expected_outputs: ExpectedOutput,
    }

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
            .map(|a| InitialStorageWrite {
                key: U256::from_dec_str(&a.key).unwrap(),
                value: a.value,
            })
            .collect();
        let auxiliary_output = BlockAuxiliaryOutput::new(
            commitment_test.auxiliary_input.l2_l1_logs.clone(),
            initial_writes,
            commitment_test.auxiliary_input.repeated_writes.clone(),
        );

        let commitment = BlockCommitment {
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
