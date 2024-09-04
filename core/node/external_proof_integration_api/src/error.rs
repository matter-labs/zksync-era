use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_basic_types::L1BatchNumber;
use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProcessorError {
    #[error("Failed to deserialize proof data")]
    Serialization(#[from] bincode::Error),
    #[error("Invalid proof submitted")]
    InvalidProof,
    #[error("Batch {0} is not yet ready for proving. Most likely our proof for this batch is not generated yet, try again later")]
    BatchNotReady(L1BatchNumber),
    #[error("Invalid file: {0}")]
    InvalidFile(#[from] FileError),
    #[error("Internal error")]
    Internal,
    #[error("Proof verification not possible anymore, batch is too old")]
    ProofIsGone,
}

impl ProcessorError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Serialization(_) => StatusCode::BAD_REQUEST,
            Self::InvalidProof => StatusCode::BAD_REQUEST,
            Self::InvalidFile(_) => StatusCode::BAD_REQUEST,
            Self::BatchNotReady(_) => StatusCode::NOT_FOUND,
            Self::ProofIsGone => StatusCode::GONE,
        }
    }
}

impl IntoResponse for ProcessorError {
    fn into_response(self) -> Response {
        (self.status_code(), self.to_string()).into_response()
    }
}

impl From<ObjectStoreError> for ProcessorError {
    fn from(err: ObjectStoreError) -> Self {
        match err {
            ObjectStoreError::KeyNotFound(_) => {
                tracing::debug!("Too old proof was requested: {:?}", err);
                Self::ProofIsGone
            }
            _ => {
                tracing::warn!("GCS error: {:?}", err);
                Self::Internal
            }
        }
    }
}

impl From<DalError> for ProcessorError {
    fn from(_err: DalError) -> Self {
        // We don't want to check if the error is `RowNotFound`: we check that batch exists before
        // processing a request, so it's handled separately.
        // Thus, any unhandled error from DAL is an internal error.
        Self::Internal
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FileError {
    #[error("Multipart error: {0}")]
    MultipartRejection(#[from] axum::extract::multipart::MultipartRejection),
    #[error("Multipart error: {0}")]
    Multipart(#[from] axum::extract::multipart::MultipartError),
    #[error("File not found in request. It was expected to be in the field {field_name} with the content type {content_type}")]
    FileNotFound {
        field_name: &'static str,
        content_type: &'static str,
    },
}
