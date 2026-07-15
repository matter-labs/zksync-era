use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

use crate::metrics::{ProcessorErrorKind, METRICS};

#[derive(Debug, thiserror::Error)]
pub enum AirbenderProcessorError {
    #[error("General error: {0:#}")]
    GeneralError(#[from] anyhow::Error),
    #[error("GCS error: {context}: {source}")]
    ObjectStore {
        source: ObjectStoreError,
        context: String,
    },
    #[error("Failed fetching/saving from db: {0}")]
    Dal(#[from] DalError),
}

impl AirbenderProcessorError {
    pub fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    fn metric_kind(&self) -> ProcessorErrorKind {
        match self {
            Self::GeneralError(_) => ProcessorErrorKind::General,
            Self::ObjectStore { .. } => ProcessorErrorKind::ObjectStore,
            Self::Dal(_) => ProcessorErrorKind::Dal,
        }
    }
}

impl IntoResponse for AirbenderProcessorError {
    fn into_response(self) -> Response {
        METRICS.airbender_processor_errors[&self.metric_kind()].inc();
        tracing::error!("{}: {}", self, self.status_code());
        (self.status_code(), self.to_string()).into_response()
    }
}
