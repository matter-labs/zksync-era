use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_basic_types::L1BatchNumber;
use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

pub(crate) enum ProcessorError {
    ObjectStore(ObjectStoreError),
    Dal(DalError),
    Serialization(bincode::Error),
    ProofNotReady(L1BatchNumber),
    InvalidProof,
}

impl From<ObjectStoreError> for ProcessorError {
    fn from(err: ObjectStoreError) -> Self {
        Self::ObjectStore(err)
    }
}

impl From<DalError> for ProcessorError {
    fn from(err: DalError) -> Self {
        Self::Dal(err)
    }
}

impl From<bincode::Error> for ProcessorError {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err)
    }
}

impl IntoResponse for ProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            ProcessorError::ObjectStore(err) => {
                tracing::error!("GCS error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed fetching/saving from GCS".to_owned(),
                )
            }
            ProcessorError::Dal(err) => {
                tracing::error!("Sqlx error: {:?}", err);
                match err.inner() {
                    zksync_dal::SqlxError::RowNotFound => {
                        (StatusCode::NOT_FOUND, "Non existing L1 batch".to_owned())
                    }
                    _ => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed fetching/saving from db".to_owned(),
                    ),
                }
            }
            ProcessorError::Serialization(err) => {
                tracing::error!("Serialization error: {:?}", err);
                (
                    StatusCode::BAD_REQUEST,
                    "Failed to deserialize proof data".to_owned(),
                )
            }
            ProcessorError::ProofNotReady(l1_batch_number) => {
                tracing::error!(
                "Proof for {l1_batch_number:?} wasn't found. Most likely it is not generated yet"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Proof for {l1_batch_number:?} wasn't found. Most likely it is not generated yet, try again later"),
                )
            }
            ProcessorError::InvalidProof => {
                tracing::error!("Invalid proof data");
                (StatusCode::BAD_REQUEST, "Invalid proof data".to_owned())
            }
        };
        (status_code, message).into_response()
    }
}
