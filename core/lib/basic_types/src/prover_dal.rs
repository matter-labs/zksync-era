//! Types exposed by the prover DAL for general-purpose use.
use std::{net::IpAddr, ops::Add};

use chrono::{DateTime, Duration, Utc};

use crate::{basic_fri_types::AggregationRound, L1BatchNumber};

// This currently lives in `zksync_prover_types` -- we don't want a dependency between prover types (`zkevm_test_harness`) and DAL.
// This will be gone as part of 1.5.0, when EIP4844 becomes normal jobs, rather than special cased ones.
pub const EIP_4844_CIRCUIT_ID: u8 = 255;

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

#[derive(Debug, Default)]
pub struct ProverJobStatusFailed {
    pub started_at: DateTime<Utc>,
    pub error: String,
}

#[derive(Debug)]
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

#[derive(Debug, Default)]
pub struct ProverJobStatusInProgress {
    pub started_at: DateTime<Utc>,
}

#[derive(Debug)]
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

#[derive(Debug, Copy, Clone)]
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
