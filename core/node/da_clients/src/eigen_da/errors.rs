use anyhow::Error;

#[derive(Debug, PartialEq)]
pub enum MemStoreError {
    BlobToLarge,
    BlobAlreadyExists,
    IncorrectCommitment,
    BlobNotFound,
}

impl Into<Error> for MemStoreError {
    fn into(self) -> Error {
        match self {
            MemStoreError::BlobToLarge => Error::msg("Blob too large"),
            MemStoreError::BlobAlreadyExists => Error::msg("Blob already exists"),
            MemStoreError::IncorrectCommitment => Error::msg("Incorrect commitment"),
            MemStoreError::BlobNotFound => Error::msg("Blob not found"),
        }
    }
}

#[derive(Debug)]
pub enum EigenDAError {
    TlsError,
    UriError,
    ConnectionError(tonic::transport::Error),
    PutError,
    GetError,
}

impl Into<Error> for EigenDAError {
    fn into(self) -> Error {
        match self {
            EigenDAError::TlsError => Error::msg("Tls error"),
            EigenDAError::UriError => Error::msg("Uri error"),
            EigenDAError::ConnectionError(e) => Error::new(e),
            EigenDAError::PutError => Error::msg("Put error"),
            EigenDAError::GetError => Error::msg("Get error"),
        }
    }
}
