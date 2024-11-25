use std::path::PathBuf;

use smart_config::{DescribeConfig, DeserializeConfig};

/// Configuration for the object store
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ObjectStoreConfig {
    #[config(flatten)]
    pub mode: ObjectStoreMode,
    #[config(default_t = 5)]
    pub max_retries: u16,
    /// Path to local directory that will be used to mirror store objects locally. If not specified, no mirroring will be used.
    /// The directory layout is identical to [`ObjectStoreMode::FileBacked`].
    ///
    /// Mirroring is primarily useful for local development and testing; it might not provide substantial performance benefits
    /// if the Internet connection used by the app is fast enough.
    ///
    /// **Important.** Mirroring logic assumes that objects in the underlying store are immutable. If this is not the case,
    /// the mirrored objects may become stale.
    pub local_mirror_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "mode", derive(Default))]
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
    #[config(default)]
    FileBacked {
        #[config(default_t = "./artifacts".into())]
        file_backed_base_path: PathBuf,
    },
}
