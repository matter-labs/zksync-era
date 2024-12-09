use std::{path::PathBuf, time::Duration};

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Configuration for the fri proof compressor
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProofCompressorConfig {
    /// The compression mode to use
    #[config(default_t = 1)]
    pub compression_mode: u8,

    // Configurations for prometheus
    pub prometheus_pushgateway_url: String,
    #[config(default_t = Duration::from_millis(100), with = TimeUnit::Millis)]
    pub prometheus_push_interval_ms: Duration,

    /// Max time for proof compression to be performed
    #[config(default_t = Duration::from_secs(3_600), with = TimeUnit::Seconds)]
    pub generation_timeout_in_secs: Duration,
    /// Max attempts for proof compression to be performed
    #[config(default_t = 5)]
    pub max_attempts: u32,

    /// Path to universal setup key file
    pub universal_setup_path: PathBuf,
    /// https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^24.key
    pub universal_setup_download_url: String,

    /// Whether to verify wrapper proof or not.
    #[config(default_t = true)]
    pub verify_wrapper_proof: bool,
}
