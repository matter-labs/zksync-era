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
