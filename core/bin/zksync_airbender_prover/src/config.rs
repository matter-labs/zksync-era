use std::time::Duration;

use anyhow::Context as _;
use secp256k1::SecretKey;
use smart_config::{
    de::Serde, ConfigRepository, ConfigSchema, DescribeConfig, DeserializeConfig, Environment, Json,
};
use url::Url;
use zksync_basic_types::tee_types::TeeType;
use zksync_config::configs::{ObservabilityConfig, PrometheusConfig};
use zksync_vlog::ObservabilityGuard;

/// Configuration for the Airbender prover.
#[derive(Debug, Clone)]
pub(crate) struct AirbenderProverConfig {
    /// Signing parameters passed via environment
    pub sig_conf: AirbenderProverSigConfig,
    /// Airbender proof data handler API.
    pub prover_api: AirbenderProverApiConfig,
}

/// Signing parameters passed via environment
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub(crate) struct AirbenderProverSigConfig {
    /// The private key used to sign the proofs.
    #[config(secret, with = Serde![str])]
    pub signing_key: SecretKey,
    #[config(default_t = TeeType::Sgx, with = Serde![str])]
    pub tee_type: TeeType,
}

/// Airbender proof data handler API parameter.
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub(crate) struct AirbenderProverApiConfig {
    /// Airbender proof data handler API.
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
    pub prometheus: PrometheusConfig,
    pub prover: AirbenderProverConfig,
}

const DEFAULT_INSTANCE_METADATA_BASE_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/attributes/container_config";

impl AppConfig {
    fn full_schema() -> ConfigSchema {
        // `unwrap()`s are safe; we know that config locations don't conflict
        let mut schema = ConfigSchema::new(&AirbenderProverSigConfig::DESCRIPTION, "airbender_prover");
        schema
            .insert(&AirbenderProverApiConfig::DESCRIPTION, "airbender_prover")
            .unwrap()
            .push_alias("prover_api")
            .unwrap();
        schema
            .insert(&ObservabilityConfig::DESCRIPTION, "observability")
            .unwrap();
        schema
            .insert(&PrometheusConfig::DESCRIPTION, "prometheus")
            .unwrap();
        schema
    }

    pub(crate) async fn try_new() -> anyhow::Result<(Self, ObservabilityGuard)> {
        let metadata = if std::env::var_os("GOOGLE_METADATA").is_some() {
            let meta_config = reqwest::Client::default()
                .get(DEFAULT_INSTANCE_METADATA_BASE_URL)
                .header("Metadata-Flavor", "Google")
                .send()
                .await
                .context("get metadata")?
                .json()
                .await
                .context("convert metadata to config")?;
            Some(Json::new("google_metadata", meta_config))
        } else {
            None
        };
        Self::from_sources(Environment::prefixed(""), metadata)
    }

    fn from_sources(
        env: Environment,
        metadata: Option<Json>,
    ) -> anyhow::Result<(Self, ObservabilityGuard)> {
        let schema = Self::full_schema();
        let mut config_repo = ConfigRepository::new(&schema).with(env);
        if let Some(metadata) = metadata {
            config_repo = config_repo.with(metadata);
        }
        let mut config_repo = zksync_config::ConfigRepository::from(config_repo);

        let observability_config: ObservabilityConfig = config_repo.parse()?;
        let observability_guard = observability_config
            .install()
            .context("installing observability failed")?;

        let this = Self {
            prometheus: config_repo.parse()?,
            prover: AirbenderProverConfig {
                sig_conf: config_repo.parse()?,
                prover_api: config_repo.parse()?,
            },
        };
        Ok((this, observability_guard))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test] // Observability can only be installed in the Tokio runtime context
    async fn test_response() {
        // TODO: return OTel config once it doesn't hang up the test
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
            "log_format" : "plain"
          },
          "prometheus" : {
            "listener_port": 3321
          },
          "prover_api" : {
            "api_url" : "http://prover_api/",
            "max_retries" : 10,
            "initial_retry_backoff_sec" : 10,
            "retry_backoff_multiplier" : 2.0,
            "max_backoff_sec" : 128
          }
        }"#;
        let json: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(mock_data).unwrap();
        let json = Json::new("google_metadata", json);

        let env = r#"
            TEE_PROVER_SIGNING_KEY="b50b38c8d396c88728fc032ece558ebda96907a0b1a9340289715eef7bf29deb"
            TEE_PROVER_TEE_TYPE="sgx"
        "#;
        let env = Environment::from_dotenv("test.env", env).unwrap();

        let (app_config, _guard) = AppConfig::from_sources(env, Some(json)).unwrap();
        assert_eq!(
            app_config.prover.prover_api.api_url.as_str(),
            "http://prover_api/"
        );
        assert_eq!(app_config.prover.prover_api.max_retries, 10);
        assert_eq!(app_config.prover.sig_conf.tee_type, TeeType::Sgx);
        assert_eq!(app_config.prometheus.listener_port, Some(3_321));
    }
}
