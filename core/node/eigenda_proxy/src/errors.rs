use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug, PartialEq)]
pub enum MemStoreError {
    BlobToLarge,
    IncorrectString,
    BlobAlreadyExists,
    IncorrectCommitment,
    BlobNotFound,
}

#[derive(Debug)]
pub enum EigenDAError {
    TlsError,
    UriError,
    ConnectionError(tonic::transport::Error),
    PutError,
    GetError,
}

pub(crate) enum RequestProcessorError {
    EigenDA(EigenDAError),
}

impl IntoResponse for RequestProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            RequestProcessorError::EigenDA(err) => {
                tracing::error!("EigenDA error: {:?}", err);
                match err {
                    EigenDAError::TlsError => (StatusCode::BAD_GATEWAY, "Tls error".to_owned()),
                    EigenDAError::UriError => (StatusCode::BAD_GATEWAY, "Uri error".to_owned()),
                    EigenDAError::ConnectionError(err) => (
                        StatusCode::BAD_GATEWAY,
                        format!("Connection error: {:?}", err).to_owned(),
                    ),
                    EigenDAError::PutError => (StatusCode::BAD_GATEWAY, "Put error".to_owned()),
                    EigenDAError::GetError => (StatusCode::BAD_GATEWAY, "Get error".to_owned()),
                }
            }
        };
        (status_code, message).into_response()
    }
}
