use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use zksync_proof_data_handler::ProcessorError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApiError {
    #[error("Processor error: {0}")]
    Processor(#[from] ProcessorError),
    #[error("Invalid file: {0}")]
    InvalidFile(#[from] FileError),
}

impl ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Processor(err) => err.status_code(),
            Self::InvalidFile(_) => StatusCode::BAD_REQUEST,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status_code(), self.to_string()).into_response()
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
