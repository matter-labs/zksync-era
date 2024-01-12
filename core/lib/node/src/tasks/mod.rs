pub mod metadata_calculator;
pub mod prometheus_exporter;

#[derive(thiserror::Error, Debug)]
pub enum TaskInitError {
    #[error("Resource {0} is not provided")]
    ResourceLacking(&'static str),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
