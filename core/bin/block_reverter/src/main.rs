use std::env;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::io::{self, AsyncReadExt};
use zksync_block_reverter::{BlockReverter, BlockReverterEthConfig, NodeRole};
use zksync_config::{
    configs::{chain::NetworkConfig, ObservabilityConfig},
    ContractsConfig, DBConfig, EthConfig, PostgresConfig,
};
use zksync_dal::{ConnectionPool, Core};
use zksync_env_config::{object_store::SnapshotsObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{Address, L1BatchNumber, U256};

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "Block revert utility", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Displays suggested values to use.
    #[command(name = "print-suggested-values")]
    Display {
        /// Displays the values as a JSON object, so that they are machine-readable.
        #[arg(long)]
        json: bool,
        /// Operator address.
        #[arg(long = "operator-address")]
        operator_address: Address,
    },
    /// Sends revert transaction to L1.
    #[command(name = "send-eth-transaction")]
    SendEthTransaction {
        /// L1 batch number used to revert to.
        #[arg(long)]
        l1_batch_number: u32,
        /// Priority fee used for the reverting Ethereum transaction.
        // We operate only by priority fee because we want to use base fee from Ethereum
        // and send transaction as soon as possible without any resend logic
        #[arg(long)]
        priority_fee_per_gas: Option<u64>,
        /// Nonce used for reverting Ethereum transaction.
        #[arg(long)]
        nonce: u64,
    },

    /// Rolls back internal database state to a previous L1 batch.
    #[command(name = "rollback-db")]
    RollbackDB {
        /// L1 batch number used to roll back to.
        #[arg(long)]
        l1_batch_number: u32,
        /// Flag that specifies if Postgres DB should be rolled back.
        #[arg(long)]
        rollback_postgres: bool,
        /// Flag that specifies if RocksDB with tree should be rolled back.
        #[arg(long)]
        rollback_tree: bool,
        /// Flag that specifies if RocksDB with state keeper cache should be rolled back.
        #[arg(long)]
        rollback_sk_cache: bool,
        /// Flag that allows to roll back already executed blocks. It's ultra dangerous and required only for fixing external nodes.
        #[arg(long)]
        allow_executed_block_reversion: bool,
    },

    /// Clears failed L1 transactions.
    #[command(name = "clear-failed-transactions")]
    ClearFailedL1Transactions,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = Cli::parse().command;
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = observability_config.sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    let eth_sender = EthConfig::from_env().context("EthConfig::from_env()")?;
    let db_config = DBConfig::from_env().context("DBConfig::from_env()")?;
    let default_priority_fee_per_gas = U256::from(
        eth_sender
            .gas_adjuster
            .context("gas_adjuster")?
            .default_priority_fee_per_gas,
    );
    let contracts = ContractsConfig::from_env().context("ContractsConfig::from_env()")?;
    let network = NetworkConfig::from_env().context("NetworkConfig::from_env()")?;
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let era_chain_id = env::var("CONTRACTS_ERA_CHAIN_ID")
        .context("`CONTRACTS_ERA_CHAIN_ID` env variable is not set")?
        .parse()
        .map_err(|err| {
            anyhow::anyhow!("failed parsing `CONTRACTS_ERA_CHAIN_ID` env variable: {err}")
        })?;
    let config = BlockReverterEthConfig::new(eth_sender, &contracts, &network, era_chain_id)?;

    let connection_pool = ConnectionPool::<Core>::builder(
        postgres_config.master_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;
    let mut block_reverter = BlockReverter::new(NodeRole::Main, connection_pool);

    match command {
        Command::Display {
            json,
            operator_address,
        } => {
            let suggested_values = block_reverter
                .suggested_values(&config, operator_address)
                .await?;
            if json {
                println!("{}", serde_json::to_string(&suggested_values)?);
            } else {
                println!("Suggested values for reversion: {:#?}", suggested_values);
            }
        }
        Command::SendEthTransaction {
            l1_batch_number,
            priority_fee_per_gas,
            nonce,
        } => {
            let priority_fee_per_gas =
                priority_fee_per_gas.map_or(default_priority_fee_per_gas, U256::from);
            block_reverter
                .send_ethereum_revert_transaction(
                    &config,
                    L1BatchNumber(l1_batch_number),
                    priority_fee_per_gas,
                    nonce,
                )
                .await?;
        }
        Command::RollbackDB {
            l1_batch_number,
            rollback_postgres,
            rollback_tree,
            rollback_sk_cache,
            allow_executed_block_reversion,
        } => {
            if !rollback_tree && rollback_postgres {
                println!("You want to roll back Postgres DB without rolling back tree.");
                println!(
                    "If the tree is not yet rolled back to this L1 batch, then the only way \
                     to make it synced with Postgres will be to completely rebuild it."
                );
                println!("Are you sure? Print y/n");

                let mut input = [0u8];
                io::stdin().read_exact(&mut input).await.unwrap();
                if input[0] != b'y' && input[0] != b'Y' {
                    std::process::exit(0);
                }
            }

            if allow_executed_block_reversion {
                println!("You want to roll back already executed blocks. It's impossible to restore them for the main node");
                println!("Make sure you are doing it ONLY for external node");
                println!("Are you sure? Print y/n");

                let mut input = [0u8];
                io::stdin().read_exact(&mut input).await.unwrap();
                if input[0] != b'y' && input[0] != b'Y' {
                    std::process::exit(0);
                }
                block_reverter.allow_rolling_back_executed_batches();
            }

            if rollback_postgres {
                block_reverter.enable_rolling_back_postgres();
                let object_store_config = SnapshotsObjectStoreConfig::from_env()
                    .context("SnapshotsObjectStoreConfig::from_env()")?;
                block_reverter.enable_rolling_back_snapshot_objects(
                    ObjectStoreFactory::new(object_store_config.0)
                        .create_store()
                        .await,
                );
            }
            if rollback_tree {
                block_reverter.enable_rolling_back_merkle_tree(db_config.merkle_tree.path);
            }
            if rollback_sk_cache {
                block_reverter
                    .enable_rolling_back_state_keeper_cache(db_config.state_keeper_db_path);
            }

            block_reverter
                .roll_back(L1BatchNumber(l1_batch_number))
                .await?;
        }
        Command::ClearFailedL1Transactions => {
            block_reverter.clear_failed_l1_transactions().await?;
        }
    }
    Ok(())
}
