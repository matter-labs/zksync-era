//! Loadtest: an utility to stress-test the zkSync server.
//!
//! In order to launch it, you must provide required environmental variables, for details see `README.md`.
//! Without required variables provided, test is launched in the localhost/development mode with some hard-coded
//! values to check the local zkSync deployment.

use tokio::sync::watch;

use std::time::Duration;

use prometheus_exporter::PrometheusExporterConfig;
use zksync_config::configs::api::PrometheusConfig;

use loadnext::{
    command::TxType,
    config::{ExecutionConfig, LoadtestConfig},
    executor::Executor,
    report_collector::LoadtestResult,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");
    let execution_config = ExecutionConfig::from_env();
    let prometheus_config: Option<PrometheusConfig> = envy::prefixed("PROMETHEUS_").from_env().ok();

    TxType::initialize_weights(&execution_config.transaction_weights);

    tracing::info!(
        "Run with tx weights: {:?}",
        execution_config.transaction_weights
    );
    let mut executor = Executor::new(config, execution_config).await?;
    let (stop_sender, stop_receiver) = watch::channel(false);

    if let Some(prometheus_config) = prometheus_config {
        let exporter_config = PrometheusExporterConfig::push(
            prometheus_config.gateway_endpoint(),
            prometheus_config.push_interval(),
        );

        tracing::info!("Starting prometheus exporter with config {prometheus_config:?}");
        tokio::spawn(exporter_config.run(stop_receiver));
    } else {
        tracing::info!("Starting without prometheus exporter");
    }

    let result = executor.start().await;
    tracing::info!("Waiting 5 seconds to make sure all the metrics are pushed to the push gateway");
    tokio::time::sleep(Duration::from_secs(5)).await;
    stop_sender.send_replace(true);

    match result {
        LoadtestResult::TestPassed => {
            tracing::info!("Test passed");
            Ok(())
        }
        LoadtestResult::TestFailed => {
            tracing::error!("Test failed");
            Err(anyhow::anyhow!("Test failed"))
        }
    }
}
