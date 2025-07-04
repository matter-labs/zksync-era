//! Types exposed by the prover DAL for general-purpose use.
use std::{ops::Add, time::Instant};

use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolVersionId, L1BatchId,
    L1BatchNumber, L2ChainId,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ProvingMode {
    ProvingNetwork,
    ProverCluster,
}

#[derive(Debug, Clone, Copy)]
pub struct FriProverJobMetadata {
    pub id: u32,
    pub batch_id: L1BatchId,
    pub batch_sealed_at: DateTime<Utc>,
    pub circuit_id: u8,
    pub aggregation_round: AggregationRound,
    pub sequence_number: usize,
    pub depth: u16,
    pub is_node_final_proof: bool,
    pub pick_time: Instant,
}

impl FriProverJobMetadata {
    /// Checks whether the metadata corresponds to a scheduler proof or not.
    pub fn is_scheduler_proof(&self) -> anyhow::Result<bool> {
        if self.aggregation_round == AggregationRound::Scheduler {
            if self.circuit_id != 1 {
                return Err(anyhow::anyhow!(
                    "Invalid circuit id {} for Scheduler proof",
                    self.circuit_id
                ));
            }
            return Ok(true);
        }
        Ok(false)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExtendedJobCountStatistics {
    pub queued: usize,
    pub in_progress: usize,
    pub failed: usize,
    pub successful: usize,
}

impl Add for ExtendedJobCountStatistics {
    type Output = ExtendedJobCountStatistics;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            queued: self.queued + rhs.queued,
            in_progress: self.in_progress + rhs.in_progress,
            failed: self.failed + rhs.failed,
            successful: self.successful + rhs.successful,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct JobCountStatistics {
    pub queued: usize,
    pub in_progress: usize,
}

impl JobCountStatistics {
    /// all returns sum of queued and in_progress.
    pub fn all(&self) -> usize {
        self.queued + self.in_progress
    }
}

#[derive(Debug)]
pub struct StuckJobs {
    pub id: u64,
    pub chain_id: L2ChainId,
    pub status: String,
    pub attempts: u64,
    pub circuit_id: Option<u32>,
    pub picked_by: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LeafAggregationJobMetadata {
    pub id: u32,
    pub batch_id: L1BatchId,
    pub circuit_id: u8,
    pub prover_job_ids_for_proofs: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct NodeAggregationJobMetadata {
    pub id: u32,
    pub batch_id: L1BatchId,
    pub circuit_id: u8,
    pub depth: u16,
    pub prover_job_ids_for_proofs: Vec<u32>,
}

#[derive(Debug)]
pub struct JobPosition {
    pub aggregation_round: AggregationRound,
    pub sequence_number: usize,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct ProverJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct ProverJobStatusSuccessful {
    pub started_at: DateTime<Utc>,
    pub time_taken: Duration,
}

impl Default for ProverJobStatusSuccessful {
    fn default() -> Self {
        ProverJobStatusSuccessful {
            started_at: DateTime::default(),
            time_taken: Duration::zero(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct ProverJobStatusInProgress {
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WitnessJobStatusSuccessful {
    pub started_at: DateTime<Utc>,
    pub time_taken: Duration,
}

impl Default for WitnessJobStatusSuccessful {
    fn default() -> Self {
        WitnessJobStatusSuccessful {
            started_at: DateTime::default(),
            time_taken: Duration::zero(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct WitnessJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

#[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr, PartialEq, Clone)]
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
    #[strum(serialize = "in_gpu_proof")]
    InGPUProof,
}

#[derive(Debug, Clone, strum::Display, strum::EnumString, strum::AsRefStr)]
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
    pub batch_id: L1BatchId,
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

#[derive(Debug, Clone)]
pub struct ProverJobFriInfo {
    pub id: u32,
    pub batch_id: L1BatchId,
    pub circuit_id: u32,
    pub circuit_blob_url: String,
    pub aggregation_round: AggregationRound,
    pub sequence_number: u32,
    pub status: ProverJobStatus,
    pub error: Option<String>,
    pub attempts: u8,
    pub processing_started_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub time_taken: Option<NaiveTime>,
    pub depth: u32,
    pub is_node_final_proof: bool,
    pub proof_blob_url: Option<String>,
    pub protocol_version: Option<ProtocolVersionId>,
    pub picked_by: Option<String>,
}

pub trait Stallable {
    fn get_status(&self) -> WitnessJobStatus;
    fn get_attempts(&self) -> u32;
}

#[derive(Debug, Clone)]
pub struct BasicWitnessGeneratorJobInfo {
    pub batch_id: L1BatchId,
    pub witness_inputs_blob_url: Option<String>,
    pub attempts: u32,
    pub status: WitnessJobStatus,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub protocol_version: Option<i32>,
    pub picked_by: Option<String>,
}

impl Stallable for BasicWitnessGeneratorJobInfo {
    fn get_status(&self) -> WitnessJobStatus {
        self.status.clone()
    }

    fn get_attempts(&self) -> u32 {
        self.attempts
    }
}

#[derive(Debug, Clone)]
pub struct LeafWitnessGeneratorJobInfo {
    pub id: u32,
    pub batch_id: L1BatchId,
    pub circuit_id: u32,
    pub closed_form_inputs_blob_url: Option<String>,
    pub attempts: u32,
    pub status: WitnessJobStatus,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub number_of_basic_circuits: Option<i32>,
    pub protocol_version: Option<i32>,
    pub picked_by: Option<String>,
}

impl Stallable for LeafWitnessGeneratorJobInfo {
    fn get_status(&self) -> WitnessJobStatus {
        self.status.clone()
    }

    fn get_attempts(&self) -> u32 {
        self.attempts
    }
}

#[derive(Debug, Clone)]
pub struct NodeWitnessGeneratorJobInfo {
    pub id: u32,
    pub batch_id: L1BatchId,
    pub circuit_id: u32,
    pub depth: u32,
    pub status: WitnessJobStatus,
    pub attempts: u32,
    pub aggregations_url: Option<String>,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub number_of_dependent_jobs: Option<i32>,
    pub protocol_version: Option<i32>,
    pub picked_by: Option<String>,
}

impl Stallable for NodeWitnessGeneratorJobInfo {
    fn get_status(&self) -> WitnessJobStatus {
        self.status.clone()
    }

    fn get_attempts(&self) -> u32 {
        self.attempts
    }
}

#[derive(Debug, Clone)]
pub struct RecursionTipWitnessGeneratorJobInfo {
    pub batch_id: L1BatchId,
    pub status: WitnessJobStatus,
    pub attempts: u32,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub number_of_final_node_jobs: i32,
    pub protocol_version: Option<i32>,
    pub picked_by: Option<String>,
}

impl Stallable for RecursionTipWitnessGeneratorJobInfo {
    fn get_status(&self) -> WitnessJobStatus {
        self.status.clone()
    }

    fn get_attempts(&self) -> u32 {
        self.attempts
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerWitnessGeneratorJobInfo {
    pub batch_id: L1BatchId,
    pub scheduler_partial_input_blob_url: String,
    pub status: WitnessJobStatus,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub attempts: u32,
    pub protocol_version: Option<i32>,
    pub picked_by: Option<String>,
}

impl Stallable for SchedulerWitnessGeneratorJobInfo {
    fn get_status(&self) -> WitnessJobStatus {
        self.status.clone()
    }

    fn get_attempts(&self) -> u32 {
        self.attempts
    }
}

#[derive(Debug, EnumString, Display, Clone, PartialEq)]
pub enum ProofCompressionJobStatus {
    #[strum(serialize = "queued")]
    Queued,
    #[strum(serialize = "in_progress")]
    InProgress,
    #[strum(serialize = "successful")]
    Successful,
    #[strum(serialize = "failed")]
    Failed,
    #[strum(serialize = "sent_to_server")]
    SentToServer,
    #[strum(serialize = "skipped")]
    Skipped,
}

#[derive(Debug, Clone)]
pub struct ProofCompressionJobInfo {
    pub batch_id: L1BatchId,
    pub attempts: u32,
    pub status: ProofCompressionJobStatus,
    pub fri_proof_blob_url: Option<String>,
    pub l1_proof_blob_url: Option<String>,
    pub error: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub processing_started_at: Option<NaiveDateTime>,
    pub time_taken: Option<NaiveTime>,
    pub picked_by: Option<String>,
}

// Used for transferring information about L1 Batches from DAL to public interfaces (currently prover_cli stats).
/// DTO containing information about L1 Batch Proof.
#[derive(Debug, Clone)]
pub struct ProofGenerationTime {
    pub batch_id: L1BatchId,
    pub time_taken: NaiveTime,
    pub created_at: NaiveDateTime,
}
