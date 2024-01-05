use std::time::Instant;
use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::io::{self, AsyncReadExt};
use zksync_config::{ContractsConfig, DBConfig, ETHClientConfig, ETHSenderConfig, PostgresConfig};
use zksync_core::basic_witness_input_producer::BasicWitnessInputProducer;
use zksync_core::block_reverter::{
    BlockReverter, BlockReverterEthConfig, BlockReverterFlags, L1ExecutedBatchesRevert,
};
use zksync_core::Component::StateKeeper;
use zksync_core::state_keeper::ZkSyncStateKeeper;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_types::{L1BatchNumber, L2ChainId, U256};

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "Batch reexecution utility", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Reexecutes block.
    #[command()]
    Reexecute
}

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
            .context("Invalid Sentry URL")?
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    // let eth_sender = ETHSenderConfig::from_env().context("ETHSenderConfig::from_env()")?;
    // let db_config = DBConfig::from_env().context("DBConfig::from_env()")?;
    // let eth_client = ETHClientConfig::from_env().context("ETHClientConfig::from_env()")?;
    // let default_priority_fee_per_gas =
    //     U256::from(eth_sender.gas_adjuster.default_priority_fee_per_gas);
    // let contracts = ContractsConfig::from_env().context("ContractsConfig::from_env()")?;
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    // let config = BlockReverterEthConfig::new(eth_sender, contracts, eth_client.web3_url.clone());

    let connection_pool = ConnectionPool::builder(
        postgres_config.replica_url()?,
        1,
    )
        .build()
        .await
        .context("failed to build a connection pool")?;

    // let rt_handle = tokio::runtime::Handle::
    let mut h = tokio::task::spawn_blocking(move || {
        let rt_handle = tokio::runtime::Handle::current();
        let wbs = BasicWitnessInputProducer::process_job_impl(
            rt_handle,
            L1BatchNumber(4954),
            Instant::now(),
            connection_pool.clone(),
            300u64.try_into().unwrap(),
        ).unwrap();
        });

    h.await;

    // match Cli::parse().command {
    //     Command::Reexecute => {
    //         let suggested_values = block_reverter.suggested_values().await;
    //         if json {
    //             println!("{}", serde_json::to_string(&suggested_values).unwrap());
    //         } else {
    //             println!("Suggested values for rollback: {:#?}", suggested_values);
    //         }
    //     }
    // }
    Ok(())
}
