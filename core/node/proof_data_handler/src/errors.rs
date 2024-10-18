use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_dal::DalError;
use zksync_object_store::ObjectStoreError;

pub(crate) enum RequestProcessorError {
    GeneralError(String),
    ObjectStore(ObjectStoreError),
    Dal(DalError),
}

impl From<DalError> for RequestProcessorError {
    fn from(err: DalError) -> Self {
        RequestProcessorError::Dal(err)
    }
}

impl IntoResponse for RequestProcessorError {
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
