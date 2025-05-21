use tee_prover::TeeProverLayer;
use zksync_node_framework::service::ZkStackServiceBuilder;
use zksync_vlog::node::{PrometheusExporterLayer, SigintHandlerLayer};

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

    let (app_config, observability_guard) =
        builder.runtime_handle().block_on(AppConfig::try_new())?;
    let prometheus_config = app_config.prometheus;

    builder
        .add_layer(SigintHandlerLayer)
        .add_layer(TeeProverLayer::new(app_config.prover));
    if let Some(exporter_config) = prometheus_config.to_exporter_config() {
        builder.add_layer(PrometheusExporterLayer(exporter_config));
    }
    builder.build().run(observability_guard)?;
    Ok(())
}
