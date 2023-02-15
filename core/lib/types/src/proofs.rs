use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::ops::Add;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::bellman::plonk::better_better_cs::proof::Proof;
use zkevm_test_harness::witness::full_block_artifact::{
    BlockBasicCircuits, BlockBasicCircuitsPublicInputs,
};
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zkevm_test_harness::{
    LeafAggregationOutputDataWitness, NodeAggregationOutputDataWitness,
    SchedulerCircuitInstanceWitness,
};
use zksync_basic_types::{L1BatchNumber, U256};

/// Metadata emitted by merkle tree after processing single storage log
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageLogMetadata {
    pub root_hash: Vec<u8>,
    pub is_write: bool,
    pub first_write: bool,
    pub merkle_paths: Vec<Vec<u8>>,
    pub leaf_hashed_key: U256,
    pub leaf_enumeration_index: u64,
    pub value_written: [u8; 32],
    pub value_read: [u8; 32],
}

impl StorageLogMetadata {
    pub fn leaf_hashed_key_array(&self) -> [u8; 32] {
        let mut result = [0u8; 32];
        self.leaf_hashed_key.to_little_endian(&mut result);
        result
    }

    pub fn merkle_paths_array(&self) -> Box<[[u8; 32]; 256]> {
        let vec_of_arrays = self
            .merkle_paths
            .clone()
            .into_iter()
            .map(|vec| TryInto::<[u8; 32]>::try_into(vec).unwrap())
            .collect::<Vec<[u8; 32]>>();

        Box::new(TryInto::<[[u8; 32]; 256]>::try_into(vec_of_arrays).unwrap())
    }
}

#[derive(Clone)]
pub struct WitnessGeneratorJobMetadata {
    pub block_number: L1BatchNumber,
    pub proofs: Vec<Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
}

#[derive(Clone)]
pub struct WitnessGeneratorJob {
    pub block_number: L1BatchNumber,
    pub job: WitnessGeneratorJobInput,
}

/// Represents the sequential number of the proof aggregation round.
/// Mostly used to be stored in `aggregation_round` column  in `prover_jobs` table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregationRound {
    BasicCircuits = 0,
    LeafAggregation = 1,
    NodeAggregation = 2,
    Scheduler = 3,
}

impl AggregationRound {
    pub fn next(&self) -> Option<AggregationRound> {
        match self {
            AggregationRound::BasicCircuits => Some(AggregationRound::LeafAggregation),
            AggregationRound::LeafAggregation => Some(AggregationRound::NodeAggregation),
            AggregationRound::NodeAggregation => Some(AggregationRound::Scheduler),
            AggregationRound::Scheduler => None,
        }
    }
}

impl TryFrom<i32> for AggregationRound {
    type Error = ();

    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            x if x == AggregationRound::BasicCircuits as i32 => Ok(AggregationRound::BasicCircuits),
            x if x == AggregationRound::LeafAggregation as i32 => {
                Ok(AggregationRound::LeafAggregation)
            }
            x if x == AggregationRound::NodeAggregation as i32 => {
                Ok(AggregationRound::NodeAggregation)
            }
            x if x == AggregationRound::Scheduler as i32 => Ok(AggregationRound::Scheduler),
            _ => Err(()),
        }
    }
}

/// Represents a job for one of the aggregation rounds.
/// `Box` is required by the linter, as the enum variants have vastly different memory footprints.
#[derive(Clone)]
pub enum WitnessGeneratorJobInput {
    BasicCircuits(Box<PrepareBasicCircuitsJob>),
    LeafAggregation(Box<PrepareLeafAggregationCircuitsJob>),
    NodeAggregation(Box<PrepareNodeAggregationCircuitJob>),
    Scheduler(Box<PrepareSchedulerCircuitJob>),
}

impl WitnessGeneratorJobInput {
    pub fn aggregation_round(&self) -> AggregationRound {
        match self {
            WitnessGeneratorJobInput::BasicCircuits(_) => AggregationRound::BasicCircuits,
            WitnessGeneratorJobInput::LeafAggregation(_) => AggregationRound::LeafAggregation,
            WitnessGeneratorJobInput::NodeAggregation(_) => AggregationRound::NodeAggregation,
            WitnessGeneratorJobInput::Scheduler(_) => AggregationRound::Scheduler,
        }
    }
}

#[derive(Clone)]
pub struct PrepareBasicCircuitsJob {
    pub merkle_paths: Vec<StorageLogMetadata>,
    pub next_enumeration_index: u64,
}

/// Enriched `PrepareBasicCircuitsJob`. All the other fields are taken from the `l1_batches` table.
#[derive(Clone)]
pub struct BasicCircuitWitnessGeneratorInput {
    pub block_number: L1BatchNumber,
    pub previous_block_timestamp: u64,
    pub block_timestamp: u64,
    pub used_bytecodes_hashes: Vec<U256>,
    pub initial_heap_content: Vec<(usize, U256)>,
    pub merkle_paths_input: PrepareBasicCircuitsJob,
}

#[derive(Clone)]
pub struct PrepareLeafAggregationCircuitsJob {
    pub basic_circuits: BlockBasicCircuits<Bn256>,
    pub basic_circuits_inputs: BlockBasicCircuitsPublicInputs<Bn256>,
    pub basic_circuits_proofs: Vec<Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
}

#[derive(Clone)]
pub struct PrepareNodeAggregationCircuitJob {
    pub previous_level_proofs: Vec<Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
    pub previous_level_leafs_aggregations: Vec<LeafAggregationOutputDataWitness<Bn256>>,
    pub previous_sequence: Vec<
        zkevm_test_harness::encodings::QueueSimulator<
            Bn256,
            zkevm_test_harness::encodings::recursion_request::RecursionRequest<Bn256>,
            2,
            2,
        >,
    >,
}

#[derive(Clone)]
pub struct PrepareSchedulerCircuitJob {
    pub incomplete_scheduler_witness: SchedulerCircuitInstanceWitness<Bn256>,
    pub final_node_aggregations: NodeAggregationOutputDataWitness<Bn256>,
    pub node_final_proof_level_proof: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    pub previous_aux_hash: [u8; 32],
    pub previous_meta_hash: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct ProverJobMetadata {
    pub id: u32,
    pub block_number: L1BatchNumber,
    pub circuit_type: String,
    pub aggregation_round: AggregationRound,
    pub sequence_number: usize,
}

pub struct ProverJob {
    pub metadata: ProverJobMetadata,
    pub circuit_input: Vec<u8>,
}

pub struct JobPosition {
    pub aggregation_round: AggregationRound,
    pub sequence_number: usize,
}

#[derive(Default)]
pub struct ProverJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

impl Default for ProverJobStatusSuccessful {
    fn default() -> Self {
        ProverJobStatusSuccessful {
            started_at: DateTime::default(),
            time_taken: chrono::Duration::zero(),
        }
    }
}

pub struct ProverJobStatusSuccessful {
    pub started_at: DateTime<Utc>,
    pub time_taken: chrono::Duration,
}

#[derive(Default)]
pub struct ProverJobStatusInProgress {
    pub started_at: DateTime<Utc>,
}

pub struct WitnessJobStatusSuccessful {
    pub started_at: DateTime<Utc>,
    pub time_taken: chrono::Duration,
}

impl Default for WitnessJobStatusSuccessful {
    fn default() -> Self {
        WitnessJobStatusSuccessful {
            started_at: DateTime::default(),
            time_taken: chrono::Duration::zero(),
        }
    }
}

#[derive(Default)]
pub struct WitnessJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

#[derive(strum::Display, strum::EnumString, strum::AsRefStr)]
pub enum ProverJobStatus {
    #[strum(serialize = "queued")]
    Queued,
    #[strum(serialize = "in_progress")]
    InProgress(ProverJobStatusInProgress),
    #[strum(serialize = "successful")]
    Successful(ProverJobStatusSuccessful),
    #[strum(serialize = "failed")]
    Failed(ProverJobStatusFailed),
    #[strum(serialize = "skipped")]
    Skipped,
    #[strum(serialize = "ignored")]
    Ignored,
}

#[derive(strum::Display, strum::EnumString, strum::AsRefStr)]
pub enum WitnessJobStatus {
    #[strum(serialize = "failed")]
    Failed(WitnessJobStatusFailed),
    #[strum(serialize = "skipped")]
    Skipped,
    #[strum(serialize = "successful")]
    Successful(WitnessJobStatusSuccessful),
    #[strum(serialize = "waiting_for_artifacts")]
    WaitingForArtifacts,
    #[strum(serialize = "waiting_for_proofs")]
    WaitingForProofs,
    #[strum(serialize = "in_progress")]
    InProgress,
    #[strum(serialize = "queued")]
    Queued,
}

pub struct WitnessJobInfo {
    pub block_number: L1BatchNumber,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: WitnessJobStatus,
    pub position: JobPosition,
}

pub struct ProverJobInfo {
    pub id: u32,
    pub block_number: L1BatchNumber,
    pub circuit_type: String,
    pub position: JobPosition,
    pub input_length: u64,
    pub status: ProverJobStatus,
    pub attempts: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct JobExtendedStatistics {
    pub successful_padding: L1BatchNumber,
    pub queued_padding: L1BatchNumber,
    pub queued_padding_len: u32,
    pub active_area: Vec<ProverJobInfo>,
}

#[derive(Debug)]
pub struct JobCountStatistics {
    pub queued: usize,
    pub in_progress: usize,
    pub failed: usize,
    pub successful: usize,
}

impl Add for JobCountStatistics {
    type Output = JobCountStatistics;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            queued: self.queued + rhs.queued,
            in_progress: self.in_progress + rhs.in_progress,
            failed: self.failed + rhs.failed,
            successful: self.successful + rhs.successful,
        }
    }
}
