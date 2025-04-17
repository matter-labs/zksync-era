use tee_prover::TeeProverLayer;
use zksync_node_framework::{
    implementations::layers::{
        prometheus_exporter::PrometheusExporterLayer, sigint::SigintHandlerLayer,
    },
    service::ZkStackServiceBuilder,
};
use zksync_vlog::prometheus::PrometheusExporterConfig;

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

    let exporter_config = if let Some(base_url) = &prometheus_config.pushgateway_url {
        let gateway_endpoint = PrometheusExporterConfig::gateway_endpoint(base_url);
        PrometheusExporterConfig::push(gateway_endpoint, prometheus_config.push_interval())
    } else {
        PrometheusExporterConfig::pull(prometheus_config.listener_port)
    };
    builder.add_layer(PrometheusExporterLayer(exporter_config));

    builder.build().run(observability_guard)?;
    Ok(())
}
