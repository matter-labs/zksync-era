use std::fmt::Debug;

use strum::{Display, EnumString};
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{
        BasicWitnessGeneratorJobInfo, LeafWitnessGeneratorJobInfo, NodeWitnessGeneratorJobInfo,
        ProofCompressionJobInfo, ProofCompressionJobStatus, ProverJobFriInfo, ProverJobStatus,
        RecursionTipWitnessGeneratorJobInfo, SchedulerWitnessGeneratorJobInfo, Stallable,
        WitnessJobStatus,
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

impl From<ProverJobStatus> for Status {
    fn from(status: ProverJobStatus) -> Self {
        match status {
            ProverJobStatus::Queued => Status::Queued,
            ProverJobStatus::InProgress(_) => Status::InProgress,
            ProverJobStatus::Successful(_) => Status::Successful,
            ProverJobStatus::Failed(_) => Status::Custom("Failed".to_owned()),
            ProverJobStatus::Skipped => Status::Custom("Skipped ⏩".to_owned()),
            ProverJobStatus::Ignored => Status::Custom("Ignored".to_owned()),
            ProverJobStatus::InGPUProof => Status::Custom("In GPU Proof".to_owned()),
        }
    }
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

    pub fn prover_jobs_status(&self, max_attempts: u32) -> Option<Status> {
        match self.clone() {
            StageInfo::BasicWitnessGenerator {
                prover_jobs_info, ..
            }
            | StageInfo::LeafWitnessGenerator {
                prover_jobs_info, ..
            }
            | StageInfo::NodeWitnessGenerator {
                prover_jobs_info, ..
            } => Some(get_prover_jobs_status_from_vec(
                &prover_jobs_info,
                max_attempts,
            )),
            StageInfo::RecursionTipWitnessGenerator(_)
            | StageInfo::SchedulerWitnessGenerator(_)
            | StageInfo::Compressor(_) => None,
        }
    }

    pub fn witness_generator_jobs_status(&self, max_attempts: u32) -> Status {
        match self.clone() {
            StageInfo::BasicWitnessGenerator {
                witness_generator_job_info,
                ..
            } => witness_generator_job_info
                .map(|witness_generator_job_info| {
                    get_witness_generator_job_status(&witness_generator_job_info, max_attempts)
                })
                .unwrap_or_default(),
            StageInfo::LeafWitnessGenerator {
                witness_generator_jobs_info,
                ..
            } => {
                get_witness_generator_job_status_from_vec(witness_generator_jobs_info, max_attempts)
            }
            StageInfo::NodeWitnessGenerator {
                witness_generator_jobs_info,
                ..
            } => {
                get_witness_generator_job_status_from_vec(witness_generator_jobs_info, max_attempts)
            }
            StageInfo::RecursionTipWitnessGenerator(witness_generator_job_info) => {
                witness_generator_job_info
                    .map(|witness_generator_job_info| {
                        get_witness_generator_job_status(&witness_generator_job_info, max_attempts)
                    })
                    .unwrap_or_default()
            }
            StageInfo::SchedulerWitnessGenerator(witness_generator_job_info) => {
                witness_generator_job_info
                    .map(|witness_generator_job_info| {
                        get_witness_generator_job_status(&witness_generator_job_info, max_attempts)
                    })
                    .unwrap_or_default()
            }
            StageInfo::Compressor(status) => status
                .map(|job| Status::from(job.status))
                .unwrap_or_default(),
        }
    }
}

pub fn get_witness_generator_job_status(data: &impl Stallable, max_attempts: u32) -> Status {
    let status = data.get_status();
    if matches!(
        status,
        WitnessJobStatus::Failed(_) | WitnessJobStatus::InProgress,
    ) && data.get_attempts() >= max_attempts
    {
        return Status::Stuck;
    }
    Status::from(status)
}

pub fn get_witness_generator_job_status_from_vec(
    prover_jobs: Vec<impl Stallable>,
    max_attempts: u32,
) -> Status {
    if prover_jobs.is_empty() {
        Status::JobsNotFound
    } else if prover_jobs.iter().any(|job| {
        matches!(
            job.get_status(),
            WitnessJobStatus::Failed(_) | WitnessJobStatus::InProgress,
        ) && job.get_attempts() as u32 >= max_attempts
    }) {
        Status::Stuck
    } else if prover_jobs
        .iter()
        .all(|job| matches!(job.get_status(), WitnessJobStatus::Queued))
    {
        Status::Queued
    } else if prover_jobs
        .iter()
        .all(|job| matches!(job.get_status(), WitnessJobStatus::Successful(_)))
    {
        Status::Successful
    } else {
        Status::InProgress
    }
}

pub fn get_prover_job_status(prover_jobs: ProverJobFriInfo, max_attempts: u32) -> Status {
    if matches!(
        prover_jobs.status,
        ProverJobStatus::Failed(_) | ProverJobStatus::InProgress(_),
    ) && prover_jobs.attempts as u32 >= max_attempts
    {
        return Status::Stuck;
    }
    Status::from(prover_jobs.status)
}

pub fn get_prover_jobs_status_from_vec(
    prover_jobs: &[ProverJobFriInfo],
    max_attempts: u32,
) -> Status {
    if prover_jobs.is_empty() {
        Status::JobsNotFound
    } else if prover_jobs.iter().any(|job| {
        matches!(
            job.status,
            ProverJobStatus::Failed(_) | ProverJobStatus::InProgress(_),
        ) && job.attempts as u32 >= max_attempts
    }) {
        Status::Stuck
    } else if prover_jobs
        .iter()
        .all(|job| matches!(job.status, ProverJobStatus::InGPUProof))
    {
        Status::Custom("In GPU Proof ⚡️".to_owned())
    } else if prover_jobs
        .iter()
        .all(|job| matches!(job.status, ProverJobStatus::Queued))
    {
        Status::Queued
    } else if prover_jobs
        .iter()
        .all(|job| matches!(job.status, ProverJobStatus::Successful(_)))
    {
        Status::Successful
    } else {
        Status::InProgress
    }
}
