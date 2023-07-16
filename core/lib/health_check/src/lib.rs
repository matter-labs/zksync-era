/// Public re-export for other crates to be able to implement the interface.
pub use async_trait::async_trait;

/// Interface to be used for healthchecks
/// There's a list of health checks that are looped in the /healthcheck endpoint to verify status
#[async_trait]
pub trait CheckHealth: Send + Sync + 'static {
    async fn check_health(&self) -> CheckHealthStatus;
}

/// Used to return health status when checked.
/// States:
///     Ready => move forward
///     NotReady => check fails with message String -- to be passed to /healthcheck caller
#[derive(Debug, PartialEq)]
pub enum CheckHealthStatus {
    Ready,
    NotReady(String),
}
