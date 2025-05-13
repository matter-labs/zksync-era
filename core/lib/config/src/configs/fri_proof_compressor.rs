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
    #[config(default_t = 1 * TimeUnit::Hours, with = TimeUnit::Seconds)]
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

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test, test_complete},
        Environment, Yaml,
    };

    use super::*;

    fn expected_config() -> FriProofCompressorConfig {
        FriProofCompressorConfig {
            compression_mode: 1,
            prometheus_pushgateway_url: "http://127.0.0.1:9091".to_string(),
            prometheus_push_interval_ms: Duration::from_millis(100),
            generation_timeout_in_secs: Duration::from_secs(3000),
            max_attempts: 5,
            universal_setup_path: "keys/setup/setup_2^26.key".into(),
            universal_setup_download_url:
                "https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^26.key"
                    .to_string(),
            verify_wrapper_proof: false,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            FRI_PROOF_COMPRESSOR_COMPRESSION_MODE=1
            FRI_PROOF_COMPRESSOR_PROMETHEUS_LISTENER_PORT=3316
            FRI_PROOF_COMPRESSOR_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            FRI_PROOF_COMPRESSOR_PROMETHEUS_PUSH_INTERVAL_MS=100
            FRI_PROOF_COMPRESSOR_GENERATION_TIMEOUT_IN_SECS=3000
            FRI_PROOF_COMPRESSOR_MAX_ATTEMPTS=5
            FRI_PROOF_COMPRESSOR_UNIVERSAL_SETUP_PATH="keys/setup/setup_2^26.key"
            FRI_PROOF_COMPRESSOR_UNIVERSAL_SETUP_DOWNLOAD_URL="https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^26.key"
            FRI_PROOF_COMPRESSOR_VERIFY_WRAPPER_PROOF=false
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("FRI_PROOF_COMPRESSOR_");

        let config: FriProofCompressorConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          compression_mode: 1
          prometheus_listener_port: 3326
          prometheus_pushgateway_url: http://127.0.0.1:9091
          prometheus_push_interval_ms: 100
          generation_timeout_in_secs: 3000
          max_attempts: 5
          universal_setup_path: keys/setup/setup_2^26.key
          universal_setup_download_url: https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^26.key
          verify_wrapper_proof: false
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriProofCompressorConfig = test(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
