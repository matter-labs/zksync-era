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

    let tee_prover_config = TeeProverConfig::from_env()?;
    let prometheus_config = PrometheusConfig::from_env()?;

    let mut builder = ZkStackServiceBuilder::new()?;
    let observability_guard = {
        // Observability initialization should be performed within tokio context.
        let _context_guard = builder.runtime_handle().enter();
        observability_config.install()?
    };

    builder
        .add_layer(SigintHandlerLayer)
        .add_layer(TeeProverLayer::new(tee_prover_config));

    if let Some(gateway) = prometheus_config.gateway_endpoint() {
        let exporter_config =
            PrometheusExporterConfig::push(gateway, prometheus_config.push_interval());
        builder.add_layer(PrometheusExporterLayer(exporter_config));
    }

    builder.build().run(observability_guard)?;
    Ok(())
}
