use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

#[derive(Debug)]
pub(crate) enum ProcessorError {
    GeneralError(String),
    ObjectStore(ObjectStoreError),
    Dal(DalError),
}

impl From<DalError> for ProcessorError {
    fn from(err: DalError) -> Self {
        ProcessorError::Dal(err)
    }
}

impl IntoResponse for ProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            Self::GeneralError(err) => {
                tracing::error!("Error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error occurred".to_owned(),
                )
            }
            Self::ObjectStore(err) => {
                tracing::error!("GCS error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from GCS".to_owned(),
                )
            }
            Self::Dal(err) => {
                tracing::error!("Sqlx error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from db".to_owned(),
                )
            }
        };
        (status_code, message).into_response()
    }
}

impl std::fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GeneralError(err) => write!(f, "General error: {}", err),
            Self::ObjectStore(err) => write!(f, "Object store error: {}", err),
            Self::Dal(err) => write!(f, "DAL error: {}", err),
        }
    }
}
