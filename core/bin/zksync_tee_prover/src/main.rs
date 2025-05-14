use tee_prover::TeeProverLayer;
use zksync_node_framework::service::ZkStackServiceBuilder;
use zksync_vlog::{
    node::{PrometheusExporterLayer, SigintHandlerLayer},
    prometheus::PrometheusExporterConfig,
};

use crate::config::AppConfig;

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
    let mut builder = ZkStackServiceBuilder::new()?;

    let AppConfig {
        observability: observability_config,
        prometheus: prometheus_config,
        prover: tee_prover_config,
    } = builder.runtime_handle().block_on(AppConfig::try_new())?;

    let observability_guard = {
        // Observability initialization should be performed within tokio context.
        let _context_guard = builder.runtime_handle().enter();
        observability_config.install()?
    };

    builder
        .add_layer(SigintHandlerLayer)
        .add_layer(TeeProverLayer::new(tee_prover_config));

    let exporter_config = if let Some(gateway) = prometheus_config.gateway_endpoint() {
        PrometheusExporterConfig::push(gateway, prometheus_config.push_interval())
    } else {
        PrometheusExporterConfig::pull(prometheus_config.listener_port)
    };
    builder.add_layer(PrometheusExporterLayer(exporter_config));

    builder.build().run(observability_guard)?;
    Ok(())
}
