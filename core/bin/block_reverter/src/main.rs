use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::io::{self, AsyncReadExt};

use zksync_config::{ContractsConfig, DBConfig, ETHClientConfig, ETHSenderConfig, PostgresConfig};
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_types::{L1BatchNumber, U256};

use zksync_core::block_reverter::{
    BlockReverter, BlockReverterEthConfig, BlockReverterFlags, L1ExecutedBatchesRevert,
};

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
    },
    /// Sends revert transaction to L1.
    #[command(name = "send-eth-transaction")]
    SendEthTransaction {
        /// L1 batch number used to rollback to.
        #[arg(long)]
        l1_batch_number: u32,
        /// Priority fee used for rollback ethereum transaction.
        // We operate only by priority fee because we want to use base fee from ethereum
        // and send transaction as soon as possible without any resend logic
        #[arg(long)]
        priority_fee_per_gas: Option<u64>,
        /// Nonce used for rollback Ethereum transaction.
        #[arg(long)]
        nonce: u64,
    },

    /// Reverts internal database state to previous block.
    #[command(name = "rollback-db")]
    RollbackDB {
        /// L1 batch number used to rollback to.
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
        /// Flag that allows to revert already executed blocks, it's ultra dangerous and required only for fixing external nodes
        #[arg(long)]
        allow_executed_block_reversion: bool,
    },

    /// Clears failed L1 transactions.
    #[command(name = "clear-failed-transactions")]
    ClearFailedL1Transactions,
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

    let eth_sender = ETHSenderConfig::from_env().context("ETHSenderConfig::from_env()")?;
    let db_config = DBConfig::from_env().context("DBConfig::from_env()")?;
    let eth_client = ETHClientConfig::from_env().context("ETHClientConfig::from_env()")?;
    let default_priority_fee_per_gas =
        U256::from(eth_sender.gas_adjuster.default_priority_fee_per_gas);
    let contracts = ContractsConfig::from_env().context("ContractsConfig::from_env()")?;
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let config = BlockReverterEthConfig::new(eth_sender, contracts, eth_client.web3_url.clone());

    let connection_pool = ConnectionPool::builder(
        postgres_config.master_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;
    let mut block_reverter = BlockReverter::new(
        db_config.state_keeper_db_path,
        db_config.merkle_tree.path,
        Some(config),
        connection_pool,
        L1ExecutedBatchesRevert::Disallowed,
    );

    match Cli::parse().command {
        Command::Display { json } => {
            let suggested_values = block_reverter.suggested_values().await;
            if json {
                println!("{}", serde_json::to_string(&suggested_values).unwrap());
            } else {
                println!("Suggested values for rollback: {:#?}", suggested_values);
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
                    L1BatchNumber(l1_batch_number),
                    priority_fee_per_gas,
                    nonce,
                )
                .await
        }
        Command::RollbackDB {
            l1_batch_number,
            rollback_postgres,
            rollback_tree,
            rollback_sk_cache,
            allow_executed_block_reversion,
        } => {
            if !rollback_tree && rollback_postgres {
                println!("You want to rollback Postgres DB without rolling back tree.");
                println!(
                    "If tree is not yet rolled back to this block then the only way \
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
                println!("You want to revert already executed blocks. It's impossible to restore them for the main node");
                println!("Make sure you are doing it ONLY for external node");
                println!("Are you sure? Print y/n");

                let mut input = [0u8];
                io::stdin().read_exact(&mut input).await.unwrap();
                if input[0] != b'y' && input[0] != b'Y' {
                    std::process::exit(0);
                }
                block_reverter.change_rollback_executed_l1_batches_allowance(
                    L1ExecutedBatchesRevert::Allowed,
                );
            }

            let mut flags = BlockReverterFlags::empty();
            if rollback_postgres {
                flags |= BlockReverterFlags::POSTGRES;
            }
            if rollback_tree {
                flags |= BlockReverterFlags::TREE;
            }
            if rollback_sk_cache {
                flags |= BlockReverterFlags::SK_CACHE;
            }
            block_reverter
                .rollback_db(L1BatchNumber(l1_batch_number), flags)
                .await
        }
        Command::ClearFailedL1Transactions => block_reverter.clear_failed_l1_transactions().await,
    }
    Ok(())
}
