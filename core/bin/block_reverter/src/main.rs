use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::{
    fs,
    io::{self, AsyncReadExt},
};
use zksync_block_reverter::{
    eth_client::{
        clients::{Client, PKSigningClient, L1},
        EthInterface,
    },
    BlockReverter, BlockReverterEthConfig, NodeRole,
};
use zksync_config::{
    configs::{
        chain::NetworkConfig, wallets::Wallets, BasicWitnessInputProducerConfig, DatabaseSecrets,
        GeneralConfig, L1Secrets, ObservabilityConfig, ProtectiveReadsWriterConfig,
    },
    ContractsConfig, DBConfig, EthConfig, GenesisConfig, PostgresConfig,
};
use zksync_core_leftovers::temp_config_store::decode_yaml_repr;
use zksync_dal::{ConnectionPool, Core};
use zksync_env_config::{object_store::SnapshotsObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{Address, L1BatchNumber};

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "Block revert utility", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Path to yaml config. If set, it will be used instead of env vars
    #[arg(long, global = true)]
    config_path: Option<PathBuf>,
    /// Path to yaml contracts config. If set, it will be used instead of env vars
    #[arg(long, global = true)]
    contracts_config_path: Option<PathBuf>,
    /// Path to yaml secrets config. If set, it will be used instead of env vars
    #[arg(long, global = true)]
    secrets_path: Option<PathBuf>,
    /// Path to yaml wallets config. If set, it will be used instead of env vars
    #[arg(long, global = true)]
    wallets_path: Option<PathBuf>,
    /// Path to yaml genesis config. If set, it will be used instead of env vars
    #[arg(long, global = true)]
    genesis_path: Option<PathBuf>,
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
        /// Flag that specifies if RocksDBs with vm runners' caches should be rolled back.
        #[arg(long)]
        rollback_vm_runners_cache: bool,
        /// Flag that specifies if snapshot files in GCS should be rolled back.
        #[arg(long, requires = "rollback_postgres")]
        rollback_snapshots: bool,
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
    let opts = Cli::parse();
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;

    let logs = zksync_vlog::Logs::try_from(observability_config.clone())
        .context("logs")?
        .disable_default_logs(); // It's a CLI application, so we only need to show logs that were actually requested.;
    let sentry: Option<zksync_vlog::Sentry> =
        TryFrom::try_from(observability_config.clone()).context("sentry")?;
    let opentelemetry: Option<zksync_vlog::OpenTelemetry> =
        TryFrom::try_from(observability_config.clone()).context("opentelemetry")?;
    let _guard = zksync_vlog::ObservabilityBuilder::new()
        .with_logs(Some(logs))
        .with_sentry(sentry)
        .with_opentelemetry(opentelemetry)
        .build();

    let general_config: Option<GeneralConfig> = if let Some(path) = opts.config_path {
        let yaml = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let config =
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
                .context("failed decoding general YAML config")?;
        Some(config)
    } else {
        None
    };
    let wallets_config: Option<Wallets> = if let Some(path) = opts.wallets_path {
        let yaml = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let config = decode_yaml_repr::<zksync_protobuf_config::proto::wallets::Wallets>(&yaml)
            .context("failed decoding wallets YAML config")?;
        Some(config)
    } else {
        None
    };
    let genesis_config: Option<GenesisConfig> = if let Some(path) = opts.genesis_path {
        let yaml = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let config = decode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&yaml)
            .context("failed decoding genesis YAML config")?;
        Some(config)
    } else {
        None
    };

    let eth_sender = match &general_config {
        Some(general_config) => general_config
            .eth
            .clone()
            .context("Failed to find eth config")?,
        None => EthConfig::from_env().context("EthConfig::from_env()")?,
    };
    let db_config = match &general_config {
        Some(general_config) => general_config
            .db_config
            .clone()
            .context("Failed to find eth config")?,
        None => DBConfig::from_env().context("DBConfig::from_env()")?,
    };
    let protective_reads_writer_config = match &general_config {
        Some(general_config) => general_config
            .protective_reads_writer_config
            .clone()
            .context("Failed to find eth config")?,
        None => ProtectiveReadsWriterConfig::from_env()
            .context("ProtectiveReadsWriterConfig::from_env()")?,
    };
    let basic_witness_input_producer_config = match &general_config {
        Some(general_config) => general_config
            .basic_witness_input_producer_config
            .clone()
            .context("Failed to find eth config")?,
        None => BasicWitnessInputProducerConfig::from_env()
            .context("BasicWitnessInputProducerConfig::from_env()")?,
    };
    let contracts = match opts.contracts_config_path {
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::contracts::Contracts>(&yaml)
                .context("failed decoding contracts YAML config")?
        }
        None => ContractsConfig::from_env().context("ContractsConfig::from_env()")?,
    };
    let secrets_config = if let Some(path) = opts.secrets_path {
        let yaml = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let config = decode_yaml_repr::<zksync_protobuf_config::proto::secrets::Secrets>(&yaml)
            .context("failed decoding secrets YAML config")?;
        Some(config)
    } else {
        None
    };

    let default_priority_fee_per_gas = eth_sender
        .gas_adjuster
        .context("gas_adjuster")?
        .default_priority_fee_per_gas;

    let database_secrets = match &secrets_config {
        Some(secrets_config) => secrets_config
            .database
            .clone()
            .context("Failed to find database config")?,
        None => DatabaseSecrets::from_env().context("DatabaseSecrets::from_env()")?,
    };
    let l1_secrets = match &secrets_config {
        Some(secrets_config) => secrets_config
            .l1
            .clone()
            .context("Failed to find l1 config")?,
        None => L1Secrets::from_env().context("L1Secrets::from_env()")?,
    };
    let postgres_config = match &general_config {
        Some(general_config) => general_config
            .postgres_config
            .clone()
            .context("Failed to find postgres config")?,
        None => PostgresConfig::from_env().context("PostgresConfig::from_env()")?,
    };
    let zksync_network_id = match &genesis_config {
        Some(genesis_config) => genesis_config.l2_chain_id,
        None => {
            NetworkConfig::from_env()
                .context("NetworkConfig::from_env()")?
                .zksync_network_id
        }
    };

    let config = BlockReverterEthConfig::new(&eth_sender, &contracts, zksync_network_id)?;

    let connection_pool = ConnectionPool::<Core>::builder(
        database_secrets.master_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;
    let mut block_reverter = BlockReverter::new(NodeRole::Main, connection_pool);

    match opts.command {
        Command::Display {
            json,
            operator_address,
        } => {
            let eth_client = Client::<L1>::http(l1_secrets.l1_rpc_url.clone())
                .context("Ethereum client")?
                .build();

            let suggested_values = block_reverter
                .suggested_values(&eth_client, &config, operator_address)
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
            let eth_client = Client::http(l1_secrets.l1_rpc_url.clone())
                .context("Ethereum client")?
                .build();
            let reverter_private_key = if let Some(wallets_config) = wallets_config {
                wallets_config
                    .eth_sender
                    .unwrap()
                    .operator
                    .private_key()
                    .to_owned()
            } else {
                #[allow(deprecated)]
                eth_sender
                    .sender
                    .context("eth_sender_config")?
                    .private_key()
                    .context("eth_sender_config.private_key")?
                    .context("eth_sender_config.private_key is not set")?
            };

            let priority_fee_per_gas = priority_fee_per_gas.unwrap_or(default_priority_fee_per_gas);
            let l1_chain_id = eth_client
                .fetch_chain_id()
                .await
                .context("cannot fetch Ethereum chain ID")?;
            let eth_client = PKSigningClient::new_raw(
                reverter_private_key,
                contracts.diamond_proxy_addr,
                priority_fee_per_gas,
                l1_chain_id,
                Box::new(eth_client),
            );

            block_reverter
                .send_ethereum_revert_transaction(
                    &eth_client,
                    &config,
                    L1BatchNumber(l1_batch_number),
                    nonce,
                )
                .await?;
        }
        Command::RollbackDB {
            l1_batch_number,
            rollback_postgres,
            rollback_tree,
            rollback_sk_cache,
            rollback_vm_runners_cache,
            rollback_snapshots,
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
                if rollback_snapshots {
                    let object_store_config = SnapshotsObjectStoreConfig::from_env()
                        .context("SnapshotsObjectStoreConfig::from_env()")?;
                    block_reverter.enable_rolling_back_snapshot_objects(
                        ObjectStoreFactory::new(object_store_config.0)
                            .create_store()
                            .await?,
                    );
                }
            }
            if rollback_tree {
                block_reverter.enable_rolling_back_merkle_tree(db_config.merkle_tree.path);
            }
            if rollback_sk_cache {
                block_reverter.add_rocksdb_storage_path_to_rollback(db_config.state_keeper_db_path);
            }

            if rollback_vm_runners_cache {
                let cache_exists = fs::try_exists(&protective_reads_writer_config.db_path)
                    .await
                    .with_context(|| {
                        format!(
                            "cannot check whether storage cache path `{}` exists",
                            protective_reads_writer_config.db_path
                        )
                    })?;
                if cache_exists {
                    block_reverter.add_rocksdb_storage_path_to_rollback(
                        protective_reads_writer_config.db_path,
                    );
                }

                let cache_exists = fs::try_exists(&basic_witness_input_producer_config.db_path)
                    .await
                    .with_context(|| {
                        format!(
                            "cannot check whether storage cache path `{}` exists",
                            basic_witness_input_producer_config.db_path
                        )
                    })?;
                if cache_exists {
                    block_reverter.add_rocksdb_storage_path_to_rollback(
                        basic_witness_input_producer_config.db_path,
                    );
                }
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
