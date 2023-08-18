use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::net::IpAddr;
use std::ops::Add;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::bellman::plonk::better_better_cs::proof::Proof;
use zkevm_test_harness::encodings::{recursion_request::RecursionRequest, QueueSimulator};
use zkevm_test_harness::witness::full_block_artifact::{
    BlockBasicCircuits, BlockBasicCircuitsPublicInputs,
};
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zkevm_test_harness::{
    LeafAggregationOutputDataWitness, NodeAggregationOutputDataWitness,
    SchedulerCircuitInstanceWitness,
};

use zksync_basic_types::{L1BatchNumber, H256, U256};

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
    // **NB.** For compatibility reasons, `#[serde_as(as = "Bytes")]` attrs are not added below.
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

#[derive(Clone)]
pub struct WitnessGeneratorJobMetadata {
    pub block_number: L1BatchNumber,
    pub proofs: Vec<Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
}

/// Represents the sequential number of the proof aggregation round.
/// Mostly used to be stored in `aggregation_round` column  in `prover_jobs` table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AggregationRound {
    BasicCircuits = 0,
    LeafAggregation = 1,
    NodeAggregation = 2,
    Scheduler = 3,
}

impl From<u8> for AggregationRound {
    fn from(item: u8) -> Self {
        match item {
            0 => AggregationRound::BasicCircuits,
            1 => AggregationRound::LeafAggregation,
            2 => AggregationRound::NodeAggregation,
            3 => AggregationRound::Scheduler,
            _ => panic!("Invalid round"),
        }
    }
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

impl FromStr for AggregationRound {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "basic_circuits" => Ok(AggregationRound::BasicCircuits),
            "leaf_aggregation" => Ok(AggregationRound::LeafAggregation),
            "node_aggregation" => Ok(AggregationRound::NodeAggregation),
            "scheduler" => Ok(AggregationRound::Scheduler),
            other => Err(format!(
                "{} is not a valid round name for witness generation",
                other
            )),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareBasicCircuitsJob {
    // Merkle paths and some auxiliary information for each read / write operation in a block.
    merkle_paths: Vec<StorageLogMetadata>,
    next_enumeration_index: u64,
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
    pub previous_sequence: Vec<QueueSimulator<Bn256, RecursionRequest<Bn256>, 2, 2>>,
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

#[derive(Debug, Clone)]
pub struct FriProverJobMetadata {
    pub id: u32,
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
    pub aggregation_round: AggregationRound,
    pub sequence_number: usize,
    pub depth: u16,
    pub is_node_final_proof: bool,
}

#[derive(Debug, Clone)]
pub struct LeafAggregationJobMetadata {
    pub id: u32,
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
    pub prover_job_ids_for_proofs: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct NodeAggregationJobMetadata {
    pub id: u32,
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
    pub depth: u16,
    pub prover_job_ids_for_proofs: Vec<u32>,
}

#[derive(Debug)]
pub struct JobPosition {
    pub aggregation_round: AggregationRound,
    pub sequence_number: usize,
}

#[derive(Debug, Default)]
pub struct ProverJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

#[derive(Debug)]
pub struct ProverJobStatusSuccessful {
    pub started_at: DateTime<Utc>,
    pub time_taken: chrono::Duration,
}

impl Default for ProverJobStatusSuccessful {
    fn default() -> Self {
        ProverJobStatusSuccessful {
            started_at: DateTime::default(),
            time_taken: chrono::Duration::zero(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ProverJobStatusInProgress {
    pub started_at: DateTime<Utc>,
}

#[derive(Debug)]
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

#[derive(Debug, Default)]
pub struct WitnessJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

#[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
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

#[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
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

#[derive(Debug)]
pub struct WitnessJobInfo {
    pub block_number: L1BatchNumber,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: WitnessJobStatus,
    pub position: JobPosition,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct JobExtendedStatistics {
    pub successful_padding: L1BatchNumber,
    pub queued_padding: L1BatchNumber,
    pub queued_padding_len: u32,
    pub active_area: Vec<ProverJobInfo>,
}

#[derive(Debug, Clone, Copy, Default)]
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

#[derive(Debug)]
pub struct StuckJobs {
    pub id: u64,
    pub status: String,
    pub attempts: u64,
}

#[derive(Debug, Clone)]
pub struct SocketAddress {
    pub host: IpAddr,
    pub port: u16,
}

#[derive(Debug)]
pub enum GpuProverInstanceStatus {
    // The instance is available for processing.
    Available,
    // The instance is running at full capacity.
    Full,
    // The instance is reserved by an synthesizer.
    Reserved,
    // The instance is not alive anymore.
    Dead,
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
