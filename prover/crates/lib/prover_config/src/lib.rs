//! Shared config definitions for prover components.

use std::path::PathBuf;

use anyhow::Context;
use smart_config::{fallback, ConfigSchema, DescribeConfig, DeserializeConfig, Serde};
use zksync_basic_types::url::SensitiveUrl;
pub use zksync_config::configs::{object_store, observability};
use zksync_config::{
    configs::{ObservabilityConfig, PrometheusConfig},
    sources::{ConfigFilePaths, ConfigSources},
};

pub use self::{
    fri_proof_compressor::FriProofCompressorConfig,
    fri_prover::FriProverConfig,
    fri_prover_gateway::FriProverGatewayConfig,
    fri_witness_generator::{FriWitnessGeneratorConfig, WitnessGenerationTimeouts},
    prover_job_monitor::ProverJobMonitorConfig,
};

mod fri_proof_compressor;
mod fri_prover;
mod fri_prover_gateway;
mod fri_witness_generator;
mod prover_job_monitor;
#[cfg(test)]
mod tests;

/// Prover-specific Postgres config.
#[derive(Debug, DescribeConfig, DeserializeConfig)]
pub struct PostgresConfig {
    /// Postgres connection string for the prover database. Not used by external nodes.
    #[config(secret, with = Serde![str])]
    pub prover_url: Option<SensitiveUrl>,
    /// Maximum size of the connection pool.
    #[config(alias = "pool_size", fallback = &fallback::Env("DATABASE_POOL_SIZE"))]
    pub max_connections: Option<u32>,
}

impl PostgresConfig {
    /// Returns a copy of the prover database URL as a `Result` to simplify error propagation.
    pub fn prover_url(&self) -> anyhow::Result<SensitiveUrl> {
        self.prover_url.clone().context("Prover DB URL is absent")
    }

    /// Returns the maximum size of the connection pool as a `Result` to simplify error propagation.
    pub fn max_connections(&self) -> anyhow::Result<u32> {
        self.max_connections.context("Max connections is absent")
    }
}

/// Configuration for all prover components.
#[derive(Debug, DescribeConfig, DeserializeConfig)]
pub struct CompleteProverConfig {
    #[config(nest, rename = "prometheus")]
    pub prometheus_config: PrometheusConfig,
    #[config(nest, rename = "postgres", alias = "database")]
    pub postgres_config: PostgresConfig,
    #[config(nest, rename = "proof_compressor", alias = "fri_proof_compressor")]
    pub proof_compressor_config: Option<FriProofCompressorConfig>,
    #[config(nest, rename = "prover", alias = "fri_prover")]
    pub prover_config: Option<FriProverConfig>,
    #[config(nest, alias = "fri_prover_gateway")]
    pub prover_gateway: Option<FriProverGatewayConfig>,
    #[config(nest, rename = "witness_generator", alias = "fri_witness")]
    pub witness_generator_config: Option<FriWitnessGeneratorConfig>,
    #[config(nest, rename = "prover_job_monitor")]
    pub prover_job_monitor_config: Option<ProverJobMonitorConfig>,
}

impl CompleteProverConfig {
    pub fn schema() -> anyhow::Result<ConfigSchema> {
        let mut schema = ConfigSchema::default();
        schema.coerce_serde_enums(true);
        schema.insert(&Self::DESCRIPTION, "")?;

        // We don't get observability config from the schema directly, but need it for docs etc.
        schema.insert(&ObservabilityConfig::DESCRIPTION, "observability")?;

        Ok(schema)
    }

    pub fn config_sources(
        config_path: Option<PathBuf>,
        secrets_path: Option<PathBuf>,
    ) -> anyhow::Result<ConfigSources> {
        Self::config_sources_inner(config_path, secrets_path, true)
    }

    fn config_sources_inner(
        config_path: Option<PathBuf>,
        secrets_path: Option<PathBuf>,
        read_env: bool,
    ) -> anyhow::Result<ConfigSources> {
        let config_file_paths = ConfigFilePaths {
            general: config_path,
            secrets: secrets_path,
            ..ConfigFilePaths::default()
        };
        config_file_paths.into_config_sources(read_env.then_some("ZKSYNC_"))
    }
}
