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
