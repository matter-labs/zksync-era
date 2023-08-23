//! Loadtest: an utility to stress-test the zkSync server.
//!
//! In order to launch it, you must provide required environmental variables, for details see `README.md`.
//! Without required variables provided, test is launched in the localhost/development mode with some hard-coded
//! values to check the local zkSync deployment.

use loadnext::{
    command::TxType,
    config::{ExecutionConfig, LoadtestConfig},
    executor::Executor,
    report_collector::LoadtestResult,
};

use std::time::Duration;
use zksync_config::configs::api::PrometheusConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vlog::init();
    let _sentry_guard = vlog::init_sentry();

    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");
    let execution_config = ExecutionConfig::from_env();
    let prometheus_config: Option<PrometheusConfig> = envy::prefixed("PROMETHEUS_").from_env().ok();

    TxType::initialize_weights(&execution_config.transaction_weights);

    vlog::info!(
        "Run with tx weights: {:?}",
        execution_config.transaction_weights
    );
    let mut executor = Executor::new(config, execution_config).await?;

    if let Some(prometheus_config) = prometheus_config {
        vlog::info!("Starting prometheus exporter with config {prometheus_config:?}");
        tokio::spawn(prometheus_exporter::run_prometheus_exporter(
            prometheus_config.listener_port,
            Some((
                prometheus_config.pushgateway_url.clone(),
                prometheus_config.push_interval(),
            )),
        ));
    } else {
        vlog::info!("Starting without prometheus exporter");
    }

    let result = executor.start().await;
    vlog::info!("Waiting 5 seconds to make sure all the metrics are pushed to the push gateway");
    tokio::time::sleep(Duration::from_secs(5)).await;

    match result {
        LoadtestResult::TestPassed => {
            vlog::info!("Test passed");
            Ok(())
        }
        LoadtestResult::TestFailed => {
            vlog::error!("Test failed");
            Err(anyhow::anyhow!("Test failed"))
        }
    }
}
