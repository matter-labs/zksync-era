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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vlog::init();

    let config = LoadtestConfig::from_env()
        .expect("Config parameters should be loaded from env or from default values");
    let execution_config = ExecutionConfig::from_env();
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

    let mut executor = Executor::new(config, execution_config).await?;
    let final_resolution = executor.start().await;

    match final_resolution {
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
