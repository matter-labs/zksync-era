use std::time::Duration;

use serde::Deserialize;

/// Configuration for the fri proof compressor
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriProofCompressorConfig {
    /// The compression mode to use
    pub compression_mode: u8,

    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,

    /// Max time for proof compression to be performed
    pub generation_timeout_in_secs: u16,
    /// Max attempts for proof compression to be performed
    pub max_attempts: u32,

    /// Path to universal setup key file
    pub universal_setup_path: String,
    /// https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^24.key
    pub universal_setup_download_url: String,

    /// Whether to verify wrapper proof or not.
    pub verify_wrapper_proof: bool,

    /// Path to bellman cuda
    /// https://github.com/matter-labs/era-bellman-cuda
    pub bellman_cuda_path: Option<String>,
}

impl FriProofCompressorConfig {
    pub fn generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }
}
