use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug, PartialEq)]
pub enum MemStoreError {
    BlobToLarge,
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

#[derive(Debug)]
pub(crate) enum RequestProcessorError {
    EigenDA(EigenDAError),
    MemStore(MemStoreError),
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
            RequestProcessorError::MemStore(err) => {
                tracing::error!("MemStore error: {:?}", err);
                match err {
                    MemStoreError::BlobToLarge => {
                        (StatusCode::BAD_REQUEST, "Blob too large".to_owned())
                    }
                    MemStoreError::BlobAlreadyExists => {
                        (StatusCode::BAD_REQUEST, "Blob already exists".to_owned())
                    }
                    MemStoreError::IncorrectCommitment => {
                        (StatusCode::BAD_REQUEST, "Incorrect commitment".to_owned())
                    }
                    MemStoreError::BlobNotFound => {
                        (StatusCode::NOT_FOUND, "Blob not found".to_owned())
                    }
                }
            }
        };
        (status_code, message).into_response()
    }
}
