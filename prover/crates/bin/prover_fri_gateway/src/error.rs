use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_object_store::ObjectStoreError;

#[derive(Debug, thiserror::Error)]
pub enum ProcessorError {
    #[error("GCS error: {0}")]
    ObjectStoreErr(#[from] ObjectStoreError),
    #[error("Database query failed: {0}")]
    DalErr(#[from] zksync_prover_dal::DalError),
}

impl ProcessorError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::ObjectStoreErr(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::DalErr(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ProcessorError {
    fn into_response(self) -> Response {
        (self.status_code(), self.to_string()).into_response()
    }
}
