use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use secp256k1::SecretKey;
use serde::Deserialize;
use url::Url;
use zksync_basic_types::tee_types::TeeType;
use zksync_config::configs::{ObservabilityConfig, PrometheusConfig};
use zksync_env_config::FromEnv;

/// Configuration for the TEE prover.
#[derive(Debug, Clone)]
pub(crate) struct TeeProverConfig {
    /// Signing parameters passed via environment
    pub sig_conf: TeeProverSigConfig,
    /// TEE proof data handler API.
    pub prover_api: TeeProverApiConfig,
}

/// Signing parameters passed via environment
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TeeProverSigConfig {
    /// The private key used to sign the proofs.
    pub signing_key: SecretKey,
    /// The path to the file containing the TEE quote.
    pub attestation_quote_file_path: PathBuf,
    /// Attestation quote file.
    pub tee_type: TeeType,
}

/// TEE proof data handler API parameter.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TeeProverApiConfig {
    /// TEE proof data handler API.
    pub api_url: Url,
    /// Number of retries for retriable errors before giving up on recovery (i.e., returning an error
    /// from [`Self::run()`]).
    #[serde(default = "TeeProverApiConfig::default_max_retries")]
    pub max_retries: usize,
    /// Initial back-off interval when retrying recovery on a retriable error. Each subsequent retry interval
    /// will be multiplied by [`Self.retry_backoff_multiplier`].
    #[serde(default = "TeeProverApiConfig::default_initial_retry_backoff_sec")]
    pub initial_retry_backoff_sec: u64,
    /// Multiplier for the back-off interval when retrying recovery on a retriable error.
    #[serde(default = "TeeProverApiConfig::default_retry_backoff_multiplier")]
    pub retry_backoff_multiplier: f32,
    /// Maximum back-off interval when retrying recovery on a retriable error.
    #[serde(default = "TeeProverApiConfig::default_max_backoff_sec")]
    pub max_backoff_sec: u64,
}

impl TeeProverApiConfig {
    pub fn initial_retry_backoff(&self) -> Duration {
        Duration::from_secs(self.initial_retry_backoff_sec)
    }

    pub fn max_backoff(&self) -> Duration {
        Duration::from_secs(self.max_backoff_sec)
    }

    pub const fn default_max_retries() -> usize {
        10
    }

    pub const fn default_initial_retry_backoff_sec() -> u64 {
        1
    }

    pub const fn default_retry_backoff_multiplier() -> f32 {
        2.0
    }

    pub const fn default_max_backoff_sec() -> u64 {
        128
    }
}

impl FromEnv for TeeProverApiConfig {
    /// Constructs the TEE Prover API configuration from environment variables.
    ///
    /// Example usage of environment variables for tests:
    /// ```
    /// export TEE_PROVER_API_URL="http://127.0.0.1:4320"
    /// export TEE_PROVER_MAX_RETRIES=10
    /// export TEE_PROVER_INITIAL_RETRY_BACKOFF_SEC=1
    /// export TEE_PROVER_RETRY_BACKOFF_MULTIPLIER=2.0
    /// export TEE_PROVER_MAX_BACKOFF_SEC=128
    /// ```
    fn from_env() -> anyhow::Result<Self> {
        let config = envy::prefixed("TEE_PROVER_").from_env()?;
        Ok(config)
    }
}

impl FromEnv for TeeProverSigConfig {
    /// Constructs the TEE Prover signature configuration from environment variables.
    ///
    /// Example usage of environment variables for tests:
    /// ```
    /// export TEE_PROVER_SIGNING_KEY="b50b38c8d396c88728fc032ece558ebda96907a0b1a9340289715eef7bf29deb"
    /// export TEE_PROVER_ATTESTATION_QUOTE_FILE_PATH="/tmp/test"  # run `echo test > /tmp/test` beforehand
    /// export TEE_PROVER_TEE_TYPE="sgx"
    /// ```
    fn from_env() -> anyhow::Result<Self> {
        let config = envy::prefixed("TEE_PROVER_").from_env()?;
        Ok(config)
    }
}

#[derive(Debug)]
pub(crate) struct AppConfig {
    pub observability: ObservabilityConfig,
    pub prometheus: PrometheusConfig,
    pub prover: TeeProverConfig,
}

const DEFAULT_INSTANCE_METADATA_BASE_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/attributes/container_config";

#[derive(Debug, Deserialize)]
pub(crate) struct MetaConfig {
    pub observability: ObservabilityConfig,
    pub prometheus: PrometheusConfig,
    pub prover_api: TeeProverApiConfig,
}

impl AppConfig {
    pub(crate) async fn try_new() -> anyhow::Result<Self> {
        let sig_conf = TeeProverSigConfig::from_env().context("TeeProverSigConfig::from_env()")?;

        if std::env::var_os("GOOGLE_METADATA").is_some() {
            let MetaConfig {
                observability,
                prometheus,
                prover_api,
            } = reqwest::Client::default()
                .get(DEFAULT_INSTANCE_METADATA_BASE_URL)
                .header("Metadata-Flavor", "Google")
                .send()
                .await
                .context("get metadata")?
                .json()
                .await
                .context("convert metadata to config")?;

            Ok(AppConfig {
                observability,
                prometheus,
                prover: TeeProverConfig {
                    sig_conf,
                    prover_api,
                },
            })
        } else {
            Ok(AppConfig {
                observability: ObservabilityConfig::from_env()
                    .context("ObservabilityConfig::from_env()")?,
                prometheus: PrometheusConfig::from_env().context("PrometheusConfig::from_env()")?,
                prover: TeeProverConfig {
                    sig_conf,
                    prover_api: TeeProverApiConfig::from_env()
                        .context("TeeProverApiConfig::from_env()")?,
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::MetaConfig;

    #[tokio::test]
    async fn test_response() {
        // Create mock response data
        let mock_data = r#"{
          "telemetry" : {
            "otlp" : {
              "enable" : true,
              "endpoint" : "http://127.0.0.1:4317",
              "protocol" : "grpc"
            },
            "logging" : {
              "level" : "trace",
              "console" : true,
              "json" : false
            }
          },
          "observability" : {
            "opentelemetry" : {
              "level": "trace",
              "endpoint" : "http://127.0.0.1:4318",
              "logs_endpoint" : "http://127.0.0.1:4318"
            },
            "log_format" : "plain"
          },
          "prometheus" : {
            "listener_port": 3321
          },
          "prover_api" : {
            "api_url" : "http://server-v2-proof-data-handler-internal.era-stage-proofs.matterlabs.corp",
            "max_retries" : 10,
            "initial_retry_backoff_sec" : 10,
            "retry_backoff_multiplier" : 2.0,
            "max_backoff_sec" : 128
          }
        }"#;
        let _meta_config: MetaConfig = serde_json::from_str(mock_data).unwrap();
    }
}
