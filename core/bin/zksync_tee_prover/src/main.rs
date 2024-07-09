use anyhow::Context as _;
use config::TeeProverConfig;
use tee_prover::TeeProverLayer;
use zksync_config::configs::{ObservabilityConfig, PrometheusConfig};
use zksync_env_config::FromEnv;
use zksync_node_framework::{
    implementations::layers::{
        prometheus_exporter::PrometheusExporterLayer, sigint::SigintHandlerLayer,
    },
    service::ZkStackServiceBuilder,
};
use zksync_vlog::prometheus::PrometheusExporterConfig;

mod api_client;
mod config;
mod error;
mod metrics;
mod tee_prover;

/// This application serves as a TEE verifier, a.k.a. a TEE prover.
///
/// - It's an application that retrieves data about batches executed by the sequencer and verifies
///   them in the TEE.
/// - It's a stateless application, e.g. it interacts with the sequencer via API and does not have
///   any kind of persistent state.
/// - It submits proofs for proven batches back to the sequencer.
/// - When the application starts, it registers the attestation on the sequencer, and then runs in a
///   loop, polling the sequencer for new jobs (batches), verifying them, and submitting generated
///   proofs back.
fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: zksync_vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;
    let mut builder = zksync_vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = observability_config.sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    let tee_prover_config = TeeProverConfig::from_env()?;
    let attestation_quote_bytes = std::fs::read(tee_prover_config.attestation_quote_file_path)?;

    let prometheus_config = PrometheusConfig::from_env()?;
    let exporter_config = PrometheusExporterConfig::push(
        prometheus_config.gateway_endpoint(),
        prometheus_config.push_interval(),
    );

    ZkStackServiceBuilder::new()
        .add_layer(SigintHandlerLayer)
        .add_layer(PrometheusExporterLayer(exporter_config))
        .add_layer(TeeProverLayer::new(
            tee_prover_config.api_url,
            tee_prover_config.signing_key,
            attestation_quote_bytes,
            tee_prover_config.tee_type,
        ))
        .build()?
        .run()?;

    Ok(())
}
