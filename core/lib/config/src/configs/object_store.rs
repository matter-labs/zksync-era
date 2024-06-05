use serde::Deserialize;

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ObjectStoreConfig {
    #[serde(flatten)]
    pub mode: ObjectStoreMode,
    #[serde(default = "ObjectStoreConfig::default_max_retries")]
    pub max_retries: u16,
    /// Path to local directory that will be used to cache objects locally. If not specified, no caching will be used.
    /// The directory layout is identical to [`ObjectStoreMode::FileBacked`].
    ///
    /// **Important.** Cache logic assumes that objects in the underlying store are immutable. If this is not the case,
    /// the cache may become stale.
    pub cache_path: Option<String>,
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
