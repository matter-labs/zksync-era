use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use secp256k1::SecretKey;
use smart_config::{
    de::Serde, ConfigRepository, ConfigSchema, DescribeConfig, DeserializeConfig, Environment, Json,
};
use url::Url;
use zksync_basic_types::tee_types::TeeType;
use zksync_config::{
    configs::{ObservabilityConfig, PrometheusConfig},
    ParseResultExt,
};

/// Configuration for the TEE prover.
#[derive(Debug, Clone)]
pub(crate) struct TeeProverConfig {
    /// Signing parameters passed via environment
    pub sig_conf: TeeProverSigConfig,
    /// TEE proof data handler API.
    pub prover_api: TeeProverApiConfig,
}

/// Signing parameters passed via environment
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub(crate) struct TeeProverSigConfig {
    /// The private key used to sign the proofs.
    #[config(secret, with = Serde![str])]
    pub signing_key: SecretKey,
    /// The path to the file containing the TEE quote.
    pub attestation_quote_file_path: PathBuf,
    /// Attestation quote file.
    #[config(default_t = TeeType::Sgx, with = Serde![str])]
    pub tee_type: TeeType,
}

/// TEE proof data handler API parameter.
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub(crate) struct TeeProverApiConfig {
    /// TEE proof data handler API.
    #[config(with = Serde![str])]
    pub api_url: Url,
    /// Number of retries for retriable errors before giving up on recovery (i.e., returning an error
    /// from [`Self::run()`]).
    #[config(default_t = 5)]
    pub max_retries: usize,
    /// Initial back-off interval when retrying recovery on a retriable error. Each subsequent retry interval
    /// will be multiplied by [`Self.retry_backoff_multiplier`].
    #[config(default_t = Duration::from_secs(1))]
    pub initial_retry_backoff: Duration,
    /// Multiplier for the back-off interval when retrying recovery on a retriable error.
    #[config(default_t = 2.0)]
    pub retry_backoff_multiplier: f32,
    /// Maximum back-off interval when retrying recovery on a retriable error.
    #[config(default_t = Duration::from_secs(128))]
    pub max_backoff: Duration,
}

#[derive(Debug)]
pub(crate) struct AppConfig {
    pub observability: ObservabilityConfig,
    pub prometheus: PrometheusConfig,
    pub prover: TeeProverConfig,
}

const DEFAULT_INSTANCE_METADATA_BASE_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/attributes/container_config";

impl AppConfig {
    pub(crate) async fn try_new() -> anyhow::Result<Self> {
        let mut schema = ConfigSchema::new(&TeeProverSigConfig::DESCRIPTION, "tee_prover");
        schema
            .insert(&TeeProverApiConfig::DESCRIPTION, "tee_prover")?
            .push_alias("prover_api")?;
        schema.insert(&ObservabilityConfig::DESCRIPTION, "observability")?;
        schema.insert(&PrometheusConfig::DESCRIPTION, "prometheus")?;

        let mut config_repo = ConfigRepository::new(&schema).with(Environment::prefixed(""));

        if std::env::var_os("GOOGLE_METADATA").is_some() {
            let meta_config: serde_json::Map<String, serde_json::Value> =
                reqwest::Client::default()
                    .get(DEFAULT_INSTANCE_METADATA_BASE_URL)
                    .header("Metadata-Flavor", "Google")
                    .send()
                    .await
                    .context("get metadata")?
                    .json()
                    .await
                    .context("convert metadata to config")?;
            let json = Json::new("google_metadata", meta_config);
            config_repo = config_repo.with(json);
        }

        // FIXME: observability should be installed here

        Ok(Self {
            observability: config_repo.single()?.parse().log_all_errors()?,
            prometheus: config_repo.single()?.parse().log_all_errors()?,
            prover: TeeProverConfig {
                sig_conf: config_repo.single()?.parse().log_all_errors()?,
                prover_api: config_repo.single()?.parse().log_all_errors()?,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Json};

    use super::*;

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
        let json: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(mock_data).unwrap();

        test_complete::<ObservabilityConfig>(Json::new("google_metadata", json.clone())).unwrap();
        test_complete::<PrometheusConfig>(Json::new("google_metadata", json.clone())).unwrap();
        test_complete::<TeeProverApiConfig>(Json::new("google_metadata", json)).unwrap();
    }
}
