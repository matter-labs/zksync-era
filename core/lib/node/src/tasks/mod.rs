mod http_api;
mod metadata_calculator;
mod prometheus_exporter;
mod state_keeper;
mod ws_api;

#[derive(thiserror::Error, Debug)]
pub enum TaskInitError {
    #[error("Resource {0} is not provided")]
    ResourceLacking(&'static str),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
