use std::fmt::Debug;

use colored::*;
use strum::{Display, EnumString};
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{ProofCompressionJobStatus, ProverJobFriInfo, ProverJobStatus, WitnessJobStatus},
    L1BatchNumber,
};

/// Represents the proving data of a batch.
pub struct BatchData {
    /// The number of the batch.
    pub batch_number: L1BatchNumber,
    /// The basic witness generator data.
    pub basic_witness_generator: Task,
    /// The leaf witness generator data.
    pub leaf_witness_generator: Task,
    /// The node witness generator data.
    pub node_witness_generator: Task,
    /// The recursion tip data.
    pub recursion_tip: Task,
    /// The scheduler data.
    pub scheduler_witness_generator: Task,
    /// The compressor data.
    pub compressor: Task,
}

impl Debug for BatchData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "== {} ==",
            format!("Batch {} Status", self.batch_number).bold()
        )?;
        writeln!(f)?;
        writeln!(f, "= {} =", "Proving Stages".to_owned().bold())?;
        writeln!(f, "{:?}", self.basic_witness_generator)?;
        writeln!(f, "{:?}", self.leaf_witness_generator)?;
        writeln!(f, "{:?}", self.node_witness_generator)?;
        writeln!(f, "{:?}", self.recursion_tip)?;
        writeln!(f, "{:?}", self.scheduler_witness_generator)?;
        writeln!(f, "{:?}", self.compressor)
    }
}

impl Default for BatchData {
    fn default() -> Self {
        BatchData {
            batch_number: L1BatchNumber::default(),
            basic_witness_generator: Task::BasicWitnessGenerator {
                status: TaskStatus::default(),
                aggregation_round_info: AggregationRoundInfo {
                    round: AggregationRound::BasicCircuits,
                    prover_jobs_status: TaskStatus::default(),
                },
            },
            leaf_witness_generator: Task::LeafWitnessGenerator {
                status: TaskStatus::default(),
                aggregation_round_info: AggregationRoundInfo {
                    round: AggregationRound::LeafAggregation,
                    prover_jobs_status: TaskStatus::default(),
                },
            },
            node_witness_generator: Task::NodeWitnessGenerator {
                status: TaskStatus::default(),
                aggregation_round_info: AggregationRoundInfo {
                    round: AggregationRound::NodeAggregation,
                    prover_jobs_status: TaskStatus::default(),
                },
            },
            recursion_tip: Task::RecursionTip {
                status: TaskStatus::default(),
                aggregation_round_info: AggregationRoundInfo {
                    round: AggregationRound::RecursionTip,
                    prover_jobs_status: TaskStatus::default(),
                },
            },
            scheduler_witness_generator: Task::SchedulerWitnessGenerator {
                status: TaskStatus::default(),
                aggregation_round_info: AggregationRoundInfo {
                    round: AggregationRound::Scheduler,
                    prover_jobs_status: TaskStatus::default(),
                },
            },
            compressor: Task::Compressor(TaskStatus::JobsNotFound),
        }
    }
}

#[derive(Default, Debug, EnumString, Clone, Display)]
pub enum TaskStatus {
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

// This implementation will change to `From<Vec<ProverJobFriInfo>>` for `AggregationRoundInfo`
// once the --verbose flag is implemented.
impl From<Vec<ProverJobFriInfo>> for TaskStatus {
    fn from(jobs_vector: Vec<ProverJobFriInfo>) -> Self {
        if jobs_vector.is_empty() {
            TaskStatus::JobsNotFound
        } else if jobs_vector
            .iter()
            .all(|job| matches!(job.status, ProverJobStatus::InGPUProof))
        {
            TaskStatus::Custom("In GPU ⚡️".to_owned())
        } else if jobs_vector
            .iter()
            .all(|job| matches!(job.status, ProverJobStatus::Queued))
        {
            TaskStatus::Queued
        } else if jobs_vector
            .iter()
            .all(|job| matches!(job.status, ProverJobStatus::Successful(_)))
        {
            TaskStatus::Successful
        } else {
            TaskStatus::InProgress
        }
    }
}

impl From<ProofCompressionJobStatus> for TaskStatus {
    fn from(status: ProofCompressionJobStatus) -> Self {
        match status {
            ProofCompressionJobStatus::Queued => TaskStatus::Queued,
            ProofCompressionJobStatus::InProgress => TaskStatus::InProgress,
            ProofCompressionJobStatus::Successful => TaskStatus::Successful,
            ProofCompressionJobStatus::Failed => TaskStatus::InProgress,
            ProofCompressionJobStatus::SentToServer => {
                TaskStatus::Custom("Sent to server 📤".to_owned())
            }
            ProofCompressionJobStatus::Skipped => TaskStatus::Custom("Skipped ⏩".to_owned()),
        }
    }
}

impl From<Vec<WitnessJobStatus>> for TaskStatus {
    fn from(status_vector: Vec<WitnessJobStatus>) -> Self {
        if status_vector.is_empty() {
            TaskStatus::JobsNotFound
        } else if status_vector
            .iter()
            .all(|job| matches!(job, WitnessJobStatus::Queued))
        {
            TaskStatus::Queued
        } else if status_vector
            .iter()
            .all(|job| matches!(job, WitnessJobStatus::WaitingForProofs))
        {
            TaskStatus::WaitingForProofs
        } else if status_vector
            .iter()
            .all(|job| matches!(job, WitnessJobStatus::Successful(_)))
        {
            TaskStatus::Successful
        } else {
            TaskStatus::InProgress
        }
    }
}

#[derive(EnumString, Clone, Display)]
pub enum Task {
    /// Represents the basic witness generator task and its status.
    #[strum(to_string = "Basic Witness Generator")]
    BasicWitnessGenerator {
        status: TaskStatus,
        aggregation_round_info: AggregationRoundInfo,
    },
    /// Represents the leaf witness generator task, its status and the aggregation round 0 prover jobs data.
    #[strum(to_string = "Leaf Witness Generator")]
    LeafWitnessGenerator {
        status: TaskStatus,
        aggregation_round_info: AggregationRoundInfo,
    },
    /// Represents the node witness generator task, its status and the aggregation round 1 prover jobs data.
    #[strum(to_string = "Node Witness Generator")]
    NodeWitnessGenerator {
        status: TaskStatus,
        aggregation_round_info: AggregationRoundInfo,
    },
    /// Represents the recursion tip task, its status and the aggregation round 2 prover jobs data.
    #[strum(to_string = "Recursion Tip")]
    RecursionTip {
        status: TaskStatus,
        aggregation_round_info: AggregationRoundInfo,
    },
    /// Represents the scheduler task and its status.
    #[strum(to_string = "Scheduler")]
    SchedulerWitnessGenerator {
        status: TaskStatus,
        aggregation_round_info: AggregationRoundInfo,
    },
    /// Represents the compressor task and its status.
    #[strum(to_string = "Compressor")]
    Compressor(TaskStatus),
}

impl Task {
    fn status(&self) -> TaskStatus {
        match self {
            Task::BasicWitnessGenerator { status, .. }
            | Task::LeafWitnessGenerator { status, .. }
            | Task::NodeWitnessGenerator { status, .. }
            | Task::RecursionTip { status, .. }
            | Task::SchedulerWitnessGenerator { status, .. }
            | Task::Compressor(status) => status.clone(),
        }
    }

    fn aggregation_round(&self) -> Option<AggregationRound> {
        match self {
            Task::BasicWitnessGenerator {
                aggregation_round_info,
                ..
            }
            | Task::LeafWitnessGenerator {
                aggregation_round_info,
                ..
            }
            | Task::NodeWitnessGenerator {
                aggregation_round_info,
                ..
            }
            | Task::RecursionTip {
                aggregation_round_info,
                ..
            }
            | Task::SchedulerWitnessGenerator {
                aggregation_round_info,
                ..
            } => Some(aggregation_round_info.round),
            Task::Compressor(_) => None,
        }
    }

    /// Returns the status of the prover jobs.
    /// If the task is not in progress or successful, returns None.
    /// Otherwise, returns the status of the prover jobs if the task
    /// has prover jobs.
    fn prover_jobs_status(&self) -> Option<TaskStatus> {
        match self {
            Task::BasicWitnessGenerator {
                status,
                aggregation_round_info,
            }
            | Task::LeafWitnessGenerator {
                status,
                aggregation_round_info,
            }
            | Task::NodeWitnessGenerator {
                status,
                aggregation_round_info,
            }
            | Task::RecursionTip {
                status,
                aggregation_round_info,
            }
            | Task::SchedulerWitnessGenerator {
                status,
                aggregation_round_info,
            } => match status {
                TaskStatus::InProgress | TaskStatus::Successful => {
                    Some(aggregation_round_info.prover_jobs_status.clone())
                }
                _ => None,
            },
            Task::Compressor(_) => None,
        }
    }
}

impl Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(aggregation_round_number) = self.aggregation_round() {
            writeln!(
                f,
                "-- {} --",
                format!("Aggregation Round {}", aggregation_round_number as u8).bold()
            )?;
            if let TaskStatus::Custom(msg) = self.status() {
                writeln!(f, "{}: {}", self.to_string().bold(), msg)?;
            } else {
                writeln!(f, "{}: {}", self.to_string().bold(), self.status())?;
            }
            if let Some(prover_jobs_status) = self.prover_jobs_status() {
                writeln!(f, "> Prover Jobs: {prover_jobs_status}")?;
            }
        } else {
            writeln!(f, "-- {} --", self.to_string().bold())?;
            writeln!(f, "{}", self.status())?;
        }
        Ok(())
    }
}

impl From<WitnessJobStatus> for TaskStatus {
    fn from(status: WitnessJobStatus) -> Self {
        match status {
            WitnessJobStatus::Queued => TaskStatus::Queued,
            WitnessJobStatus::InProgress => TaskStatus::InProgress,
            WitnessJobStatus::Successful(_) => TaskStatus::Successful,
            WitnessJobStatus::Failed(_) => TaskStatus::InProgress,
            WitnessJobStatus::WaitingForArtifacts => {
                TaskStatus::Custom("Waiting for Artifacts ⏱️".to_owned())
            }
            WitnessJobStatus::Skipped => TaskStatus::Custom("Skipped ⏩".to_owned()),
            WitnessJobStatus::WaitingForProofs => TaskStatus::WaitingForProofs,
        }
    }
}

#[derive(Clone)]
pub struct AggregationRoundInfo {
    pub round: AggregationRound,
    pub prover_jobs_status: TaskStatus,
}

impl Default for AggregationRoundInfo {
    fn default() -> Self {
        AggregationRoundInfo {
            round: AggregationRound::BasicCircuits,
            prover_jobs_status: TaskStatus::default(),
        }
    }
}
