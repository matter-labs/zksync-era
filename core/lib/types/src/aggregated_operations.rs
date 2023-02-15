use crate::commitment::BlockWithMetadata;
use crate::U256;
use codegen::serialize_proof;
use serde::{Deserialize, Serialize};
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::bellman::plonk::better_better_cs::proof::Proof;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_basic_types::{ethabi::Token, L1BatchNumber};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlocksCommitOperation {
    pub last_committed_block: BlockWithMetadata,
    pub blocks: Vec<BlockWithMetadata>,
}

impl BlocksCommitOperation {
    pub fn get_eth_tx_args(&self) -> Vec<Token> {
        let stored_block_info = self.last_committed_block.l1_header_data();
        let blocks_to_commit = self
            .blocks
            .iter()
            .map(|block| block.l1_commit_data())
            .collect();

        vec![stored_block_info, Token::Array(blocks_to_commit)]
    }

    pub fn block_range(&self) -> (L1BatchNumber, L1BatchNumber) {
        let BlocksCommitOperation { blocks, .. } = self;
        (
            blocks.first().map(|b| b.header.number).unwrap_or_default(),
            blocks.last().map(|b| b.header.number).unwrap_or_default(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlocksCreateProofOperation {
    pub blocks: Vec<BlockWithMetadata>,
    pub proofs_to_pad: usize,
}

#[derive(Clone)]
pub struct BlockProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

#[derive(Clone)]
pub struct BlocksProofOperation {
    pub prev_block: BlockWithMetadata,
    pub blocks: Vec<BlockWithMetadata>,
    pub proofs: Vec<BlockProofForL1>,
    pub should_verify: bool,
}

impl BlocksProofOperation {
    pub fn get_eth_tx_args(&self) -> Vec<Token> {
        let prev_block = self.prev_block.l1_header_data();
        let blocks_arg = Token::Array(self.blocks.iter().map(|b| b.l1_header_data()).collect());

        if self.should_verify {
            // currently we only support submitting a single proof
            assert_eq!(self.proofs.len(), 1);
            assert_eq!(self.blocks.len(), 1);

            let BlockProofForL1 {
                aggregation_result_coords,
                scheduler_proof,
            } = self.proofs.first().unwrap();

            let (_, proof) = serialize_proof(scheduler_proof);

            let proof_input = Token::Tuple(vec![
                Token::Array(
                    aggregation_result_coords
                        .iter()
                        .map(|bytes| Token::Uint(U256::from_big_endian(bytes)))
                        .collect(),
                ),
                Token::Array(proof.into_iter().map(Token::Uint).collect()),
            ]);

            vec![prev_block, blocks_arg, proof_input]
        } else {
            vec![
                prev_block,
                blocks_arg,
                Token::Tuple(vec![Token::Array(vec![]), Token::Array(vec![])]),
            ]
        }
    }

    pub fn block_range(&self) -> (L1BatchNumber, L1BatchNumber) {
        let BlocksProofOperation { blocks, .. } = self;
        (
            blocks.first().map(|c| c.header.number).unwrap_or_default(),
            blocks.last().map(|c| c.header.number).unwrap_or_default(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlocksExecuteOperation {
    pub blocks: Vec<BlockWithMetadata>,
}

impl BlocksExecuteOperation {
    fn get_eth_tx_args_for_block(block: &BlockWithMetadata) -> Token {
        block.l1_header_data()
    }

    pub fn get_eth_tx_args(&self) -> Vec<Token> {
        vec![Token::Array(
            self.blocks
                .iter()
                .map(BlocksExecuteOperation::get_eth_tx_args_for_block)
                .collect(),
        )]
    }

    pub fn block_range(&self) -> (L1BatchNumber, L1BatchNumber) {
        let BlocksExecuteOperation { blocks } = self;
        (
            blocks.first().map(|b| b.header.number).unwrap_or_default(),
            blocks.last().map(|b| b.header.number).unwrap_or_default(),
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
pub enum AggregatedActionType {
    CommitBlocks,
    PublishProofBlocksOnchain,
    ExecuteBlocks,
}

impl std::string::ToString for AggregatedActionType {
    fn to_string(&self) -> String {
        match self {
            AggregatedActionType::CommitBlocks => "CommitBlocks".to_owned(),
            AggregatedActionType::PublishProofBlocksOnchain => {
                "PublishProofBlocksOnchain".to_owned()
            }
            AggregatedActionType::ExecuteBlocks => "ExecuteBlocks".to_owned(),
        }
    }
}

impl std::str::FromStr for AggregatedActionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CommitBlocks" => Ok(Self::CommitBlocks),
            "PublishProofBlocksOnchain" => Ok(Self::PublishProofBlocksOnchain),
            "ExecuteBlocks" => Ok(Self::ExecuteBlocks),
            _ => Err("Incorrect aggregated action type".to_owned()),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum AggregatedOperation {
    CommitBlocks(BlocksCommitOperation),
    PublishProofBlocksOnchain(BlocksProofOperation),
    ExecuteBlocks(BlocksExecuteOperation),
}

impl AggregatedOperation {
    pub fn get_action_type(&self) -> AggregatedActionType {
        match self {
            AggregatedOperation::CommitBlocks(..) => AggregatedActionType::CommitBlocks,
            AggregatedOperation::PublishProofBlocksOnchain(..) => {
                AggregatedActionType::PublishProofBlocksOnchain
            }
            AggregatedOperation::ExecuteBlocks(..) => AggregatedActionType::ExecuteBlocks,
        }
    }

    pub fn get_block_range(&self) -> (L1BatchNumber, L1BatchNumber) {
        match self {
            AggregatedOperation::CommitBlocks(op) => op.block_range(),
            AggregatedOperation::PublishProofBlocksOnchain(op) => op.block_range(),
            AggregatedOperation::ExecuteBlocks(op) => op.block_range(),
        }
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            AggregatedOperation::CommitBlocks(_) => "commit",
            AggregatedOperation::PublishProofBlocksOnchain(_) => "proof",
            AggregatedOperation::ExecuteBlocks(_) => "execute",
        }
    }

    pub fn is_commit(&self) -> bool {
        matches!(self.get_action_type(), AggregatedActionType::CommitBlocks)
    }

    pub fn is_execute(&self) -> bool {
        matches!(self.get_action_type(), AggregatedActionType::ExecuteBlocks)
    }

    pub fn is_publish_proofs(&self) -> bool {
        matches!(
            self.get_action_type(),
            AggregatedActionType::PublishProofBlocksOnchain
        )
    }
}

impl From<BlocksCommitOperation> for AggregatedOperation {
    fn from(other: BlocksCommitOperation) -> Self {
        Self::CommitBlocks(other)
    }
}

impl From<BlocksProofOperation> for AggregatedOperation {
    fn from(other: BlocksProofOperation) -> Self {
        Self::PublishProofBlocksOnchain(other)
    }
}

impl From<BlocksExecuteOperation> for AggregatedOperation {
    fn from(other: BlocksExecuteOperation) -> Self {
        Self::ExecuteBlocks(other)
    }
}
