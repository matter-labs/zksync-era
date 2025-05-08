use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

#[derive(Debug, thiserror::Error)]
pub enum TeeProcessorError {
    #[error("General error: {0}")]
    GeneralError(String),
    #[error("GCS error: {context}: {source}")]
    ObjectStore {
        source: ObjectStoreError,
        context: String,
    },
    #[error("Failed fetching/saving from db: {0}")]
    Dal(#[from] DalError),
}

impl TeeProcessorError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::GeneralError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ObjectStore { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Dal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for TeeProcessorError {
    fn into_response(self) -> Response {
        tracing::error!("{}: {}", self, self.status_code());
        (self.status_code(), self.to_string()).into_response()
    }
}
