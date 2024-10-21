use axum::response::{IntoResponse, Response};

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
    ConnectionError,
    PutError,
    GetError,
}

pub(crate) enum RequestProcessorError {}

impl IntoResponse for RequestProcessorError {
    fn into_response(self) -> Response {
        unimplemented!("EigenDA request error into response")
    }
}
