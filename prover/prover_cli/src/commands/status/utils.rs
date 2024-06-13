use std::fmt::Debug;

use strum::{Display, EnumString};
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{
        BasicWitnessGeneratorJobInfo, LeafWitnessGeneratorJobInfo, NodeWitnessGeneratorJobInfo,
        ProofCompressionJobInfo, ProofCompressionJobStatus, ProverJobFriInfo, ProverJobStatus,
        RecursionTipWitnessGeneratorJobInfo, SchedulerWitnessGeneratorJobInfo, WitnessJobStatus,
    },
    L1BatchNumber,
};

/// Represents the proving data of a batch.
pub struct BatchData {
    /// The number of the batch.
    pub batch_number: L1BatchNumber,
    /// The basic witness generator data.
    pub basic_witness_generator: StageInfo,
    /// The leaf witness generator data.
    pub leaf_witness_generator: StageInfo,
    /// The node witness generator data.
    pub node_witness_generator: StageInfo,
    /// The recursion tip data.
    pub recursion_tip_witness_generator: StageInfo,
    /// The scheduler data.
    pub scheduler_witness_generator: StageInfo,
    /// The compressor data.
    pub compressor: StageInfo,
}

#[derive(Default, Debug, EnumString, Clone, Display)]
pub enum Status {
    /// A custom status that can be set manually.
    /// Mostly used when a task has singular status.
    Custom(String),
    /// A task is considered queued when all of its jobs is queued.
    #[strum(to_string = "Queued 📥")]
    Queued,
    /// A task is considered in progress when at least one of its jobs differs in its status.
    #[strum(to_string = "In Progress ⌛️")]
    InProgress,
    /// A task is considered successful when all of its jobs were processed successfully.
    #[strum(to_string = "Successful ✅")]
    Successful,
    /// A task is considered waiting for proofs when all of its jobs are waiting for proofs.
    #[strum(to_string = "Waiting for Proof ⏱️")]
    WaitingForProofs,
    /// A task is considered stuck when at least one of its jobs is stuck.
    #[strum(to_string = "Stuck ⛔️")]
    Stuck,
    /// A task has no jobs.
    #[default]
    #[strum(to_string = "Jobs not found 🚫")]
    JobsNotFound,
}

impl From<WitnessJobStatus> for Status {
    fn from(status: WitnessJobStatus) -> Self {
        match status {
            WitnessJobStatus::Queued => Status::Queued,
            WitnessJobStatus::InProgress => Status::InProgress,
            WitnessJobStatus::Successful(_) => Status::Successful,
            WitnessJobStatus::Failed(_) => Status::InProgress,
            WitnessJobStatus::WaitingForArtifacts => {
                Status::Custom("Waiting for Artifacts ⏱️".to_owned())
            }
            WitnessJobStatus::Skipped => Status::Custom("Skipped ⏩".to_owned()),
            WitnessJobStatus::WaitingForProofs => Status::WaitingForProofs,
        }
    }
}

impl From<Vec<WitnessJobStatus>> for Status {
    fn from(status_vector: Vec<WitnessJobStatus>) -> Self {
        if status_vector.is_empty() {
            Status::JobsNotFound
        } else if status_vector
            .iter()
            .all(|job| matches!(job, WitnessJobStatus::WaitingForProofs))
        {
            Status::WaitingForProofs
        } else if status_vector.iter().all(|job| {
            matches!(job, WitnessJobStatus::Queued)
                || matches!(job, WitnessJobStatus::WaitingForProofs)
        }) {
            Status::Queued
        } else if status_vector
            .iter()
            .all(|job| matches!(job, WitnessJobStatus::Successful(_)))
        {
            Status::Successful
        } else {
            Status::InProgress
        }
    }
}

impl From<Vec<LeafWitnessGeneratorJobInfo>> for Status {
    fn from(leaf_info_vector: Vec<LeafWitnessGeneratorJobInfo>) -> Self {
        leaf_info_vector
            .iter()
            .map(|s| s.status.clone())
            .collect::<Vec<WitnessJobStatus>>()
            .into()
    }
}

impl From<Vec<NodeWitnessGeneratorJobInfo>> for Status {
    fn from(node_info_vector: Vec<NodeWitnessGeneratorJobInfo>) -> Self {
        node_info_vector
            .iter()
            .map(|s| s.status.clone())
            .collect::<Vec<WitnessJobStatus>>()
            .into()
    }
}

impl From<Vec<RecursionTipWitnessGeneratorJobInfo>> for Status {
    fn from(scheduler_info_vector: Vec<RecursionTipWitnessGeneratorJobInfo>) -> Self {
        scheduler_info_vector
            .iter()
            .map(|s| s.status.clone())
            .collect::<Vec<WitnessJobStatus>>()
            .into()
    }
}

impl From<Vec<SchedulerWitnessGeneratorJobInfo>> for Status {
    fn from(scheduler_info_vector: Vec<SchedulerWitnessGeneratorJobInfo>) -> Self {
        scheduler_info_vector
            .iter()
            .map(|s| s.status.clone())
            .collect::<Vec<WitnessJobStatus>>()
            .into()
    }
}

impl From<ProofCompressionJobStatus> for Status {
    fn from(status: ProofCompressionJobStatus) -> Self {
        match status {
            ProofCompressionJobStatus::Queued => Status::Queued,
            ProofCompressionJobStatus::InProgress => Status::InProgress,
            ProofCompressionJobStatus::Successful => Status::Successful,
            ProofCompressionJobStatus::Failed => Status::InProgress,
            ProofCompressionJobStatus::SentToServer => {
                Status::Custom("Sent to server 📤".to_owned())
            }
            ProofCompressionJobStatus::Skipped => Status::Custom("Skipped ⏩".to_owned()),
        }
    }
}

impl From<Vec<ProverJobFriInfo>> for Status {
    fn from(jobs_vector: Vec<ProverJobFriInfo>) -> Self {
        if jobs_vector.is_empty() {
            Status::JobsNotFound
        } else if jobs_vector
            .iter()
            .all(|job| matches!(job.status, ProverJobStatus::InGPUProof))
        {
            Status::Custom("In GPU Proof ⚡️".to_owned())
        } else if jobs_vector
            .iter()
            .all(|job| matches!(job.status, ProverJobStatus::Queued))
        {
            Status::Queued
        } else if jobs_vector
            .iter()
            .all(|job| matches!(job.status, ProverJobStatus::Successful(_)))
        {
            Status::Successful
        } else {
            Status::InProgress
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(EnumString, Clone, Display)]
pub enum StageInfo {
    #[strum(to_string = "Basic Witness Generator")]
    BasicWitnessGenerator {
        witness_generator_job_info: Option<BasicWitnessGeneratorJobInfo>,
        prover_jobs_info: Vec<ProverJobFriInfo>,
    },
    #[strum(to_string = "Leaf Witness Generator")]
    LeafWitnessGenerator {
        witness_generator_jobs_info: Vec<LeafWitnessGeneratorJobInfo>,
        prover_jobs_info: Vec<ProverJobFriInfo>,
    },
    #[strum(to_string = "Node Witness Generator")]
    NodeWitnessGenerator {
        witness_generator_jobs_info: Vec<NodeWitnessGeneratorJobInfo>,
        prover_jobs_info: Vec<ProverJobFriInfo>,
    },
    #[strum(to_string = "Recursion Tip")]
    RecursionTipWitnessGenerator(Option<RecursionTipWitnessGeneratorJobInfo>),
    #[strum(to_string = "Scheduler")]
    SchedulerWitnessGenerator(Option<SchedulerWitnessGeneratorJobInfo>),
    #[strum(to_string = "Compressor")]
    Compressor(Option<ProofCompressionJobInfo>),
}

impl StageInfo {
    pub fn aggregation_round(&self) -> Option<AggregationRound> {
        match self {
            StageInfo::BasicWitnessGenerator { .. } => Some(AggregationRound::BasicCircuits),
            StageInfo::LeafWitnessGenerator { .. } => Some(AggregationRound::LeafAggregation),
            StageInfo::NodeWitnessGenerator { .. } => Some(AggregationRound::NodeAggregation),
            StageInfo::RecursionTipWitnessGenerator { .. } => Some(AggregationRound::RecursionTip),
            StageInfo::SchedulerWitnessGenerator { .. } => Some(AggregationRound::Scheduler),
            StageInfo::Compressor(_) => None,
        }
    }

    pub fn prover_jobs_status(&self) -> Option<Status> {
        match self.clone() {
            StageInfo::BasicWitnessGenerator {
                prover_jobs_info, ..
            }
            | StageInfo::LeafWitnessGenerator {
                prover_jobs_info, ..
            }
            | StageInfo::NodeWitnessGenerator {
                prover_jobs_info, ..
            } => Some(Status::from(prover_jobs_info)),
            StageInfo::RecursionTipWitnessGenerator(_)
            | StageInfo::SchedulerWitnessGenerator(_)
            | StageInfo::Compressor(_) => None,
        }
    }

    pub fn witness_generator_jobs_status(&self) -> Status {
        match self.clone() {
            StageInfo::BasicWitnessGenerator {
                witness_generator_job_info,
                ..
            } => witness_generator_job_info
                .map(|witness_generator_job_info| Status::from(witness_generator_job_info.status))
                .unwrap_or_default(),
            StageInfo::LeafWitnessGenerator {
                witness_generator_jobs_info,
                ..
            } => Status::from(witness_generator_jobs_info),
            StageInfo::NodeWitnessGenerator {
                witness_generator_jobs_info,
                ..
            } => Status::from(witness_generator_jobs_info),
            StageInfo::RecursionTipWitnessGenerator(status) => status
                .map(|job| Status::from(job.status))
                .unwrap_or_default(),
            StageInfo::SchedulerWitnessGenerator(status) => status
                .map(|job| Status::from(job.status))
                .unwrap_or_default(),
            StageInfo::Compressor(status) => status
                .map(|job| Status::from(job.status))
                .unwrap_or_default(),
        }
    }
}
