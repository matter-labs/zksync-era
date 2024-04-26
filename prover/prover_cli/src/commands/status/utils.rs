use std::fmt::Debug;

use colored::*;
use strum::{Display, EnumString};
use zksync_config::PostgresConfig;
use zksync_env_config::FromEnv;
use zksync_types::{
    prover_dal::{ProofCompressionJobStatus, ProverJobFriInfo, ProverJobStatus, WitnessJobStatus},
    L1BatchNumber,
};

pub fn postgres_config() -> anyhow::Result<PostgresConfig> {
    Ok(PostgresConfig::from_env()?)
}

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
    pub scheduler: Task,
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
        writeln!(f, "= {} =", format!("Proving Stages").bold())?;
        writeln!(f, "{:?}", self.basic_witness_generator)?;
        writeln!(f, "{:?}", self.leaf_witness_generator)?;
        writeln!(f, "{:?}", self.node_witness_generator)?;
        writeln!(f, "{:?}", self.recursion_tip)?;
        writeln!(f, "{:?}", self.scheduler)?;
        writeln!(f, "{:?}", self.compressor)
    }
}

impl Default for BatchData {
    fn default() -> Self {
        BatchData {
            batch_number: L1BatchNumber::default(),
            basic_witness_generator: Task::BasicWitnessGenerator {
                status: TaskStatus::WaitingForProofs,
                prover_jobs_status: TaskStatus::default(),
            },
            leaf_witness_generator: Task::LeafWitnessGenerator {
                status: TaskStatus::WaitingForProofs,
                prover_jobs_status: TaskStatus::default(),
            },
            node_witness_generator: Task::NodeWitnessGenerator {
                status: TaskStatus::WaitingForProofs,
                prover_jobs_status: TaskStatus::default(),
            },
            recursion_tip: Task::RecursionTip {
                status: TaskStatus::WaitingForProofs,
                prover_jobs_status: TaskStatus::default(),
            },
            scheduler: Task::Scheduler(TaskStatus::WaitingForProofs),
            compressor: Task::Compressor(TaskStatus::WaitingForProofs),
        }
    }
}

#[derive(Debug, EnumString, Clone, Display)]
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
    #[strum(to_string = "Stuck 🛑")]
    Stuck,
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::WaitingForProofs
    }
}

impl From<Vec<ProverJobFriInfo>> for TaskStatus {
    fn from(jobs_vector: Vec<ProverJobFriInfo>) -> Self {
        if jobs_vector.is_empty() {
            return TaskStatus::Custom("No Jobs found ".to_owned());
        }

        if jobs_vector
            .iter()
            .all(|job| job.status == ProverJobStatus::Queued)
        {
            return TaskStatus::Queued;
        } else if jobs_vector.iter().all(|job| match job.status {
            ProverJobStatus::Successful(_) => true,
            _ => false,
        }) {
            return TaskStatus::Successful;
        }

        TaskStatus::InProgress
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

#[derive(EnumString, Clone, Display)]
pub enum Task {
    /// Represents the basic witness generator task and its status.
    #[strum(to_string = "Basic Witness Generator")]
    BasicWitnessGenerator {
        status: TaskStatus,
        prover_jobs_status: TaskStatus,
    },
    /// Represents the leaf witness generator task, its status and the aggregation round 0 prover jobs data.
    #[strum(to_string = "Leaf Witness Generator")]
    LeafWitnessGenerator {
        status: TaskStatus,
        prover_jobs_status: TaskStatus,
    },
    /// Represents the node witness generator task, its status and the aggregation round 1 prover jobs data.
    #[strum(to_string = "Node Witness Generator")]
    NodeWitnessGenerator {
        status: TaskStatus,
        prover_jobs_status: TaskStatus,
    },
    /// Represents the recursion tip task, its status and the aggregation round 2 prover jobs data.
    #[strum(to_string = "Recursion Tip")]
    RecursionTip {
        status: TaskStatus,
        prover_jobs_status: TaskStatus,
    },
    /// Represents the scheduler task and its status.
    #[strum(to_string = "Scheduler")]
    Scheduler(TaskStatus),
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
            | Task::Scheduler(status)
            | Task::Compressor(status) => status.clone(),
        }
    }
}

impl Task {
    fn prover_jobs_status(&self) -> Option<TaskStatus> {
        match self {
            Task::BasicWitnessGenerator {
                prover_jobs_status, ..
            }
            | Task::LeafWitnessGenerator {
                prover_jobs_status, ..
            }
            | Task::NodeWitnessGenerator {
                prover_jobs_status, ..
            }
            | Task::RecursionTip {
                prover_jobs_status, ..
            } => Some(prover_jobs_status.clone()),
            Task::Scheduler(_) => None,
            Task::Compressor(_) => None,
        }
    }
}

impl Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "-- {} --", self.to_string().bold())?;
        match self.status() {
            TaskStatus::InProgress | TaskStatus::Successful => {
                writeln!(f, "> {}", self.status().to_string())?;
                if let Some(status) = self.prover_jobs_status() {
                    writeln!(f, "> {}", status.to_string())?;
                }
            }
            TaskStatus::Queued | TaskStatus::WaitingForProofs | TaskStatus::Stuck => {
                writeln!(f, "> {}", self.status().to_string())?
            }
            TaskStatus::Custom(msg) => writeln!(f, "> {msg}")?,
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
