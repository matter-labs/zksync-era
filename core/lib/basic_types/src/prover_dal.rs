//! Types exposed by the prover DAL for general-purpose use.
use std::{net::IpAddr, ops::Add, str::FromStr};

use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, Utc};
use strum::{Display, EnumString};

use crate::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolVersionId, L1BatchNumber,
};

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

#[derive(Debug, Clone, Copy, Default)]
pub struct ExtendedJobCountStatistics {
    pub queued: usize,
    pub in_progress: usize,
    pub failed: usize,
    pub successful: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct JobCountStatistics {
    pub queued: usize,
    pub in_progress: usize,
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

#[derive(Debug)]
pub struct StuckJobs {
    pub id: u64,
    pub status: String,
    pub attempts: u64,
    pub circuit_id: Option<u32>,
}

// TODO (PLA-774): Redundant structure, should be replaced with `std::net::SocketAddr`.
#[derive(Debug, Clone)]
pub struct SocketAddress {
    pub host: IpAddr,
    pub port: u16,
}

impl From<SocketAddress> for std::net::SocketAddr {
    fn from(socket_address: SocketAddress) -> Self {
        Self::new(socket_address.host, socket_address.port)
    }
}

impl From<std::net::SocketAddr> for SocketAddress {
    fn from(socket_address: std::net::SocketAddr) -> Self {
        Self {
            host: socket_address.ip(),
            port: socket_address.port(),
        }
    }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

impl FromStr for GpuProverInstanceStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "available" => Ok(Self::Available),
            "full" => Ok(Self::Full),
            "reserved" => Ok(Self::Reserved),
            "dead" => Ok(Self::Dead),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProverJobFriInfo {
    pub id: u32,
    pub l1_batch_number: L1BatchNumber,
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

#[derive(Debug, Clone)]
pub struct BasicWitnessGeneratorJobInfo {
    pub l1_batch_number: L1BatchNumber,
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

#[derive(Debug, Clone)]
pub struct LeafWitnessGeneratorJobInfo {
    pub id: u32,
    pub l1_batch_number: L1BatchNumber,
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

#[derive(Debug, Clone)]
pub struct NodeWitnessGeneratorJobInfo {
    pub id: u32,
    pub l1_batch_number: L1BatchNumber,
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

#[derive(Debug, Clone)]
pub struct RecursionTipWitnessGeneratorJobInfo {
    pub l1_batch_number: L1BatchNumber,
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

#[derive(Debug, Clone)]
pub struct SchedulerWitnessGeneratorJobInfo {
    pub l1_batch_number: L1BatchNumber,
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

#[derive(Debug, EnumString, Display, Clone)]
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
    pub l1_batch_number: L1BatchNumber,
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
    pub l1_batch_number: L1BatchNumber,
    pub time_taken: NaiveTime,
    pub created_at: NaiveDateTime,
}
