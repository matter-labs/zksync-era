use crate::wiring_layer::WiringError;

/// An error that can occur during the service lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum ZkStackServiceError {
    #[error("Detected a Tokio Runtime. ZkStackService manages its own runtime and does not support nested runtimes")]
    RuntimeDetected,
    #[error("No tasks have been added to the service")]
    NoTasks,
    #[error("One or more wiring layers failed to initialize: {0:?}")]
    Wiring(Vec<(String, WiringError)>),
    #[error(transparent)]
    Task(#[from] anyhow::Error),
}
