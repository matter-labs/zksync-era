use codegen::serialize_proof;

use std::{fmt, ops, str::FromStr};

use serde::{Deserialize, Serialize};
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::bellman::plonk::better_better_cs::proof::Proof;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_basic_types::{ethabi::Token, L1BatchNumber};

use crate::{commitment::L1BatchWithMetadata, ProtocolVersionId, U256};

fn l1_batch_range_from_batches(
    batches: &[L1BatchWithMetadata],
) -> ops::RangeInclusive<L1BatchNumber> {
    let start = batches
        .first()
        .map(|l1_batch| l1_batch.header.number)
        .unwrap_or_default();
    let end = batches
        .last()
        .map(|l1_batch| l1_batch.header.number)
        .unwrap_or_default();
    start..=end
}

#[derive(Debug, Clone)]
pub struct L1BatchCommitOperation {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl L1BatchCommitOperation {
    pub fn get_eth_tx_args(&self) -> Vec<Token> {
        let stored_batch_info = self.last_committed_l1_batch.l1_header_data();
        let l1_batches_to_commit = self
            .l1_batches
            .iter()
            .map(L1BatchWithMetadata::l1_commit_data)
            .collect();

        vec![stored_batch_info, Token::Array(l1_batches_to_commit)]
    }

    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        l1_batch_range_from_batches(&self.l1_batches)
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchCreateProofOperation {
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs_to_pad: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

impl fmt::Debug for L1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchProofForL1")
            .field("aggregation_result_coords", &self.aggregation_result_coords)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchProofOperation {
    pub prev_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs: Vec<L1BatchProofForL1>,
    pub should_verify: bool,
}

impl L1BatchProofOperation {
    pub fn get_eth_tx_args(&self) -> Vec<Token> {
        let prev_l1_batch = self.prev_l1_batch.l1_header_data();
        let batches_arg = self
            .l1_batches
            .iter()
            .map(L1BatchWithMetadata::l1_header_data)
            .collect();
        let batches_arg = Token::Array(batches_arg);

        if self.should_verify {
            // currently we only support submitting a single proof
            assert_eq!(self.proofs.len(), 1);
            assert_eq!(self.l1_batches.len(), 1);

            let L1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof,
            } = self.proofs.first().unwrap();

            let (_, proof) = serialize_proof(scheduler_proof);

            let aggregation_result_coords = if self.l1_batches[0]
                .header
                .protocol_version
                .unwrap()
                .is_pre_boojum()
            {
                Token::Array(
                    aggregation_result_coords
                        .iter()
                        .map(|bytes| Token::Uint(U256::from_big_endian(bytes)))
                        .collect(),
                )
            } else {
                Token::Array(Vec::new())
            };
            let proof_input = Token::Tuple(vec![
                aggregation_result_coords,
                Token::Array(proof.into_iter().map(Token::Uint).collect()),
            ]);

            vec![prev_l1_batch, batches_arg, proof_input]
        } else {
            vec![
                prev_l1_batch,
                batches_arg,
                Token::Tuple(vec![Token::Array(vec![]), Token::Array(vec![])]),
            ]
        }
    }

    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        l1_batch_range_from_batches(&self.l1_batches)
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchExecuteOperation {
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl L1BatchExecuteOperation {
    pub fn get_eth_tx_args(&self) -> Vec<Token> {
        vec![Token::Array(
            self.l1_batches
                .iter()
                .map(L1BatchWithMetadata::l1_header_data)
                .collect(),
        )]
    }

    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        l1_batch_range_from_batches(&self.l1_batches)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregatedActionType {
    Commit,
    PublishProofOnchain,
    Execute,
}

impl AggregatedActionType {
    pub fn as_str(self) -> &'static str {
        // "Blocks" suffixes are there for legacy reasons
        match self {
            Self::Commit => "CommitBlocks",
            Self::PublishProofOnchain => "PublishProofBlocksOnchain",
            Self::Execute => "ExecuteBlocks",
        }
    }
}

impl fmt::Display for AggregatedActionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AggregatedActionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CommitBlocks" => Ok(Self::Commit),
            "PublishProofBlocksOnchain" => Ok(Self::PublishProofOnchain),
            "ExecuteBlocks" => Ok(Self::Execute),
            _ => Err(
                "Incorrect aggregated action type; expected one of `CommitBlocks`, `PublishProofBlocksOnchain`, \
                `ExecuteBlocks`",
            ),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AggregatedOperation {
    Commit(L1BatchCommitOperation),
    PublishProofOnchain(L1BatchProofOperation),
    Execute(L1BatchExecuteOperation),
}

impl AggregatedOperation {
    pub fn get_action_type(&self) -> AggregatedActionType {
        match self {
            Self::Commit(_) => AggregatedActionType::Commit,
            Self::PublishProofOnchain(_) => AggregatedActionType::PublishProofOnchain,
            Self::Execute(_) => AggregatedActionType::Execute,
        }
    }

    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        match self {
            Self::Commit(op) => op.l1_batch_range(),
            Self::PublishProofOnchain(op) => op.l1_batch_range(),
            Self::Execute(op) => op.l1_batch_range(),
        }
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            Self::Commit(_) => "commit",
            Self::PublishProofOnchain(_) => "proof",
            Self::Execute(_) => "execute",
        }
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        match self {
            Self::Commit(op) => op.l1_batches[0].header.protocol_version.unwrap(),
            Self::PublishProofOnchain(op) => op.l1_batches[0].header.protocol_version.unwrap(),
            Self::Execute(op) => op.l1_batches[0].header.protocol_version.unwrap(),
        }
    }
}
