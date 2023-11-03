use serde::Deserialize;

#[derive(Debug, Deserialize, Eq, PartialEq, Clone, Copy)]
pub enum ObjectStoreMode {
    GCS,
    GCSWithCredentialFile,
    FileBacked,
}

/// Configuration for the object store
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ObjectStoreConfig {
    pub bucket_base_url: String,
    pub mode: ObjectStoreMode,
    pub file_backed_base_path: String,
    pub gcs_credential_file_path: String,
    pub max_retries: u16,
}
