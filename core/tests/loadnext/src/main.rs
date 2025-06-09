//! Loadtest: an utility to stress-test the ZKsync server.
//!
//! In order to launch it, you must provide required environmental variables, for details see `README.md`.
//! Without required variables provided, test is launched in the localhost/development mode with some hard-coded
//! values to check the local ZKsync deployment.

use std::time::Duration;

use anyhow::Context as _;
use loadnext::{
    command::TxType,
    config::{ExecutionConfig, LoadtestConfig, PrometheusConfig},
    executor::Executor,
    report_collector::LoadtestResult,
};
use tokio::sync::watch;
use zksync_vlog::prometheus::PrometheusExporterConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_format: zksync_vlog::logs::LogFormat = std::env::var("MISC_LOG_FORMAT")
        .ok()
        .as_deref()
        .unwrap_or("plain")
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

    let logs = zksync_vlog::Logs::new(log_format);
    let sentry = sentry_url
        .map(|url| {
            anyhow::Ok(
                zksync_vlog::Sentry::new(&url)
                    .context("Invalid Sentry URL")?
                    .with_environment(environment),
            )
        })
        .transpose()?;
    let _guard = zksync_vlog::ObservabilityBuilder::new()
        .with_logs(Some(logs))
        .with_sentry(sentry)
        .build();

    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");
    let execution_config = ExecutionConfig::from_env();
    let prometheus_config = PrometheusConfig::from_env();

    TxType::initialize_weights(&execution_config.transaction_weights);

    tracing::info!(
        "Run with tx weights: {:?}",
        execution_config.transaction_weights
    );
    let mut executor = Executor::new(config, execution_config).await?;
    let (stop_sender, stop_receiver) = watch::channel(false);

    if let Some(config) = prometheus_config {
        let gateway_endpoint = PrometheusExporterConfig::gateway_endpoint(&config.pushgateway_url);
        let push_interval = Duration::from_millis(config.push_interval_ms);
        tracing::info!("Starting prometheus exporter with gateway {gateway_endpoint:?} and push_interval {push_interval:?}");
        let exporter_config = PrometheusExporterConfig::push(gateway_endpoint, push_interval);
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
