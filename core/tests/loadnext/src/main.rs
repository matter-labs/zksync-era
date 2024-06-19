//! Loadtest: an utility to stress-test the ZKsync server.
//!
//! In order to launch it, you must provide required environmental variables, for details see `README.md`.
//! Without required variables provided, test is launched in the localhost/development mode with some hard-coded
//! values to check the local ZKsync deployment.

use std::time::Duration;

use loadnext::{
    command::TxType,
    config::{ExecutionConfig, LoadtestConfig},
    executor::Executor,
    report_collector::LoadtestResult,
};
use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::watch;
use zksync_config::configs::api::PrometheusConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // We don't want to introduce dependency on `zksync_env_config` in loadnext,
    // but we historically rely on the environment variables for the observability configuration,
    // so we load them directly here.
    let log_format: vlog::LogFormat = std::env::var("MISC_LOG_FORMAT")
        .ok()
        .unwrap_or("plain".to_string())
        .parse()?;
    let sentry_url = std::env::var("MISC_SENTRY_URL")
        .ok()
        .filter(|s| s != "unset");
    let environment = {
        let l1_network = std::env::var("CHAIN_ETH_NETWORK").ok();
        let l2_network = std::env::var("CHAIN_ETH_ZKSYNC_NETWORK").ok();
        match (l1_network, l2_network) {
            (Some(l1_network), Some(l2_network)) => {
                Some(format!("{} - {}", l1_network, l2_network))
            }
            _ => None,
        }
    };

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
