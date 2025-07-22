use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_dal::DalError;
use zksync_object_store::{ObjectStoreError, _reexports::BoxedError};
use zksync_types::L1BatchNumber;

#[derive(Debug, thiserror::Error)]
pub enum ProcessorError {
    #[error("General error: {0}")]
    GeneralError(String),
    #[error("GCS error: {0}")]
    ObjectStore(#[from] ObjectStoreError),
    #[error("Failed to deserialize proof data")]
    Serialization(#[from] BoxedError),
    #[error("Invalid proof submitted")]
    InvalidProof,
    #[error("Batch {0} is not yet ready for proving. Most likely our proof for this batch is not generated yet, try again later")]
    BatchNotReady(L1BatchNumber),
    #[error("Internal error")]
    Internal,
    #[error("Proof verification not possible anymore, batch is too old")]
    ProofIsGone,
}

impl ProcessorError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::GeneralError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ObjectStore(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Serialization(_) => StatusCode::BAD_REQUEST,
            Self::InvalidProof => StatusCode::BAD_REQUEST,
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

impl From<DalError> for ProcessorError {
    fn from(_err: DalError) -> Self {
        // We don't want to check if the error is `RowNotFound`: we check that batch exists before
        // processing a request, so it's handled separately.
        // Thus, any unhandled error from DAL is an internal error.
        Self::Internal
    }
}
