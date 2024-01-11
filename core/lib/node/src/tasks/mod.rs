pub mod http_api;
pub mod metadata_calculator;
pub mod prometheus_exporter;
pub mod state_keeper;
pub mod ws_api;

#[derive(thiserror::Error, Debug)]
pub enum TaskInitError {
    #[error("Resource {0} is not provided")]
    ResourceLacking(&'static str),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
