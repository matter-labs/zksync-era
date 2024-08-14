use serde::{Deserialize, Serialize};

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectStoreConfig {
    #[serde(flatten)]
    pub mode: ObjectStoreMode,
    #[serde(default = "ObjectStoreConfig::default_max_retries")]
    pub max_retries: u16,
    /// Path to local directory that will be used to mirror store objects locally. If not specified, no mirroring will be used.
    /// The directory layout is identical to [`ObjectStoreMode::FileBacked`].
    ///
    /// Mirroring is primarily useful for local development and testing; it might not provide substantial performance benefits
    /// if the Internet connection used by the app is fast enough.
    ///
    /// **Important.** Mirroring logic assumes that objects in the underlying store are immutable. If this is not the case,
    /// the mirrored objects may become stale.
    pub local_mirror_path: Option<String>,
}

impl ObjectStoreConfig {
    const fn default_max_retries() -> u16 {
        5
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
