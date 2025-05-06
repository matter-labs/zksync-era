use std::fmt;

use crate::{task::TaskId, wiring_layer::WiringError};

/// An error that can occur during the task lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Task {0} failed: {1:#}")]
    TaskFailed(TaskId, anyhow::Error),
    #[error("Task {0} panicked: {1}")]
    TaskPanicked(TaskId, String),
    #[error("Shutdown for task {0} timed out")]
    TaskShutdownTimedOut(TaskId),
    #[error("Shutdown hook {0} failed: {1:#}")]
    ShutdownHookFailed(TaskId, anyhow::Error),
    #[error("Shutdown hook {0} timed out")]
    ShutdownHookTimedOut(TaskId),
}

/// Wrapper of a list of errors with a reasonable formatting.
pub struct TaskErrors(pub Vec<TaskError>);

impl From<Vec<TaskError>> for TaskErrors {
    fn from(errs: Vec<TaskError>) -> Self {
        Self(errs)
    }
}

impl fmt::Debug for TaskErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0
            .iter()
            .map(|err| format!("{err:#}"))
            .collect::<Vec<_>>()
            .fmt(f)
    }
}

/// An error that can occur during the service lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum ZkStackServiceError {
    #[error("Detected a Tokio Runtime. ZkStackService manages its own runtime and does not support nested runtimes")]
    RuntimeDetected,
    #[error("No tasks have been added to the service")]
    NoTasks,
    #[error("One or more wiring layers failed to initialize: {0:?}")]
    Wiring(Vec<(String, WiringError)>),
    #[error("One or more tasks failed: {0:?}")]
    Task(TaskErrors),
}
