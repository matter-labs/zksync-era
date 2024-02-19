use serde::Deserialize;

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ObjectStoreConfig {
    #[serde(flatten)]
    pub mode: ObjectStoreMode,
    #[serde(default = "ObjectStoreConfig::default_max_retries")]
    pub max_retries: u16,
}

impl ObjectStoreConfig {
    const fn default_max_retries() -> u16 {
        5
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "mode")]
pub enum ObjectStoreMode {
    GCS {
        bucket_base_url: String,
    },
    GCSAnonymousReadOnly {
        bucket_base_url: String,
    },
    GCSWithCredentialFile {
        bucket_base_url: String,
        gcs_credential_file_path: String,
    },
    FileBacked {
        file_backed_base_path: String,
    },
}
