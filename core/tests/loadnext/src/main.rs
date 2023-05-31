//! Loadtest: an utility to stress-test the zkSync server.
//!
//! In order to launch it, you must provide required environmental variables, for details see `README.md`.
//! Without required variables provided, test is launched in the localhost/development mode with some hard-coded
//! values to check the local zkSync deployment.

use loadnext::{
    command::{ExplorerApiRequestType, TxType},
    config::{ExecutionConfig, LoadtestConfig},
    executor::Executor,
    report_collector::LoadtestResult,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _sentry_guard = vlog::init();

    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");
    let execution_config = ExecutionConfig::from_env();
    let prometheus_config = envy::prefixed("PROMETHEUS_").from_env().ok();

    TxType::initialize_weights(&execution_config.transaction_weights);
    ExplorerApiRequestType::initialize_weights(&execution_config.explorer_api_config_weights);

    vlog::info!(
        "Run with tx weights: {:?}",
        execution_config.transaction_weights
    );

    vlog::info!(
        "Run explorer api weights: {:?}",
        execution_config.explorer_api_config_weights
    );

    let mut executor = Executor::new(config.clone(), execution_config).await?;

    if let Some(prometheus_config) = prometheus_config {
        vlog::info!(
            "Starting prometheus exporter with config {:?}",
            prometheus_config
        );
        tokio::spawn(prometheus_exporter::run_prometheus_exporter(
            prometheus_config,
            true,
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
