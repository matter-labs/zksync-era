use std::{str::FromStr, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        ContractsConfig, FriProofCompressorConfig, FriProverConfig, FriProverGatewayConfig,
        FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig, ObservabilityConfig,
        PrometheusConfig, ProofDataHandlerConfig,
    },
    ApiConfig, ContractVerifierConfig, DBConfig, EthConfig, EthWatchConfig, GasAdjusterConfig,
    GenesisConfig, ObjectStoreConfig, PostgresConfig, SnapshotsCreatorConfig,
};
use zksync_core::{
    genesis, genesis_init, initialize_components, is_genesis_needed, setup_sigint_handler,
    temp_config_store::{decode_yaml, decode_yaml_repr, Secrets, TempConfigStore},
    Component, Components,
};
use zksync_env_config::FromEnv;
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::ManagedTasks;

mod config;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "zkSync operator node", long_about = None)]
struct Cli {
    /// Generate genesis block for the first contract deployment using temporary DB.
    #[arg(long)]
    genesis: bool,
    /// Rebuild tree.
    #[arg(long)]
    rebuild_tree: bool,
    /// Comma-separated list of components to launch.
    #[arg(
        long,
        default_value = "api,tree,eth,state_keeper,housekeeper,basic_witness_input_producer,commitment_generator"
    )]
    components: ComponentsToRun,
    /// Path to the yaml config. If set, it will be used instead of env vars.
    #[arg(long)]
    config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with secrets. If set, it will be used instead of env vars.
    #[arg(long)]
    secrets_path: Option<std::path::PathBuf>,
    /// Path to the yaml with contracts. If set, it will be used instead of env vars.
    #[arg(long)]
    contracts_config_path: Option<std::path::PathBuf>,
    /// Path to the wallets config. If set, it will be used instead of env vars.
    #[arg(long)]
    wallets_path: Option<std::path::PathBuf>,
    /// Path to the yaml with genesis. If set, it will be used instead of env vars.
    #[arg(long)]
    genesis_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
struct ComponentsToRun(Vec<Component>);

impl FromStr for ComponentsToRun {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let components = s.split(',').try_fold(vec![], |mut acc, component_str| {
            let components = Components::from_str(component_str.trim())?;
            acc.extend(components.0);
            Ok::<_, String>(acc)
        })?;
        Ok(Self(components))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();
    let sigint_receiver = setup_sigint_handler();

    // Load env config and use it if file config is not provided
    let tmp_config = load_env_config()?;

    let configs = match opt.config_path {
        None => tmp_config.general(),
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
                .context("failed decoding general YAML config")?
        }
    };

    let observability_config = configs
        .observability
        .clone()
        .context("observability config")?;

    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(log_directives) = observability_config.log_directives {
        builder = builder.with_log_directives(log_directives);
    }

    if let Some(sentry_url) = &observability_config.sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let wallets = match opt.wallets_path {
        None => tmp_config.wallets(),
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::wallets::Wallets>(&yaml)
                .context("failed decoding wallets YAML config")?
        }
    };

    let secrets: Secrets = match opt.secrets_path {
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml(&yaml).context("failed decoding secrets YAML config")?
        }
        None => Secrets {
            consensus: config::read_consensus_secrets().context("read_consensus_secrets()")?,
        },
    };

    let consensus = config::read_consensus_config().context("read_consensus_config()")?;

    let contracts_config = match opt.contracts_config_path {
        None => ContractsConfig::from_env().context("contracts_config")?,
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::contracts::Contracts>(&yaml)
                .context("failed decoding contracts YAML config")?
        }
    };

    let genesis = match opt.genesis_path {
        None => GenesisConfig::from_env().context("Genesis config")?,
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&yaml)
                .context("failed decoding genesis YAML config")?
        }
    };

    let postgres_config = configs.postgres_config.clone().context("PostgresConfig")?;

    if opt.genesis || is_genesis_needed(&postgres_config).await {
        genesis_init(genesis.clone(), &postgres_config)
            .await
            .context("genesis_init")?;

        if let Some(ecosystem_contracts) = &contracts_config.ecosystem_contracts {
            let eth_client = configs.eth.as_ref().context("eth config")?;
            genesis::save_set_chain_id_tx(
                &eth_client.web3_url,
                contracts_config.diamond_proxy_addr,
                ecosystem_contracts.state_transition_proxy_addr,
                &postgres_config,
            )
            .await
            .context("Failed to save SetChainId upgrade transaction")?;
        }

        if opt.genesis {
            return Ok(());
        }
    }

    let components = if opt.rebuild_tree {
        vec![Component::Tree]
    } else {
        opt.components.0
    };

    // Run core actors.
    let (core_task_handles, stop_sender, health_check_handle) = initialize_components(
        &configs,
        &wallets,
        &genesis,
        &contracts_config,
        &components,
        &secrets,
        consensus,
    )
    .await
    .context("Unable to start Core actors")?;

    tracing::info!("Running {} core task handlers", core_task_handles.len());

    let mut tasks = ManagedTasks::new(core_task_handles);
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = sigint_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
    }

    stop_sender.send(true).ok();
    tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .context("error waiting for RocksDB instances to drop")?;
    let complete_timeout =
        if components.contains(&Component::HttpApi) || components.contains(&Component::WsApi) {
            // Increase timeout because of complicated graceful shutdown procedure for API servers.
            Duration::from_secs(30)
        } else {
            Duration::from_secs(5)
        };
    tasks.complete(complete_timeout).await;
    health_check_handle.stop().await;
    tracing::info!("Stopped");
    Ok(())
}

fn load_env_config() -> anyhow::Result<TempConfigStore> {
    Ok(TempConfigStore {
        postgres_config: PostgresConfig::from_env().ok(),
        health_check_config: HealthCheckConfig::from_env().ok(),
        merkle_tree_api_config: MerkleTreeApiConfig::from_env().ok(),
        web3_json_rpc_config: Web3JsonRpcConfig::from_env().ok(),
        circuit_breaker_config: CircuitBreakerConfig::from_env().ok(),
        mempool_config: MempoolConfig::from_env().ok(),
        network_config: NetworkConfig::from_env().ok(),
        contract_verifier: ContractVerifierConfig::from_env().ok(),
        operations_manager_config: OperationsManagerConfig::from_env().ok(),
        state_keeper_config: StateKeeperConfig::from_env().ok(),
        house_keeper_config: HouseKeeperConfig::from_env().ok(),
        fri_proof_compressor_config: FriProofCompressorConfig::from_env().ok(),
        fri_prover_config: FriProverConfig::from_env()
            .context("fri_prover_config")
            .ok(),
        fri_prover_group_config: FriProverGroupConfig::from_env().ok(),
        fri_prover_gateway_config: FriProverGatewayConfig::from_env().ok(),
        fri_witness_vector_generator: FriWitnessVectorGeneratorConfig::from_env().ok(),
        fri_witness_generator_config: FriWitnessGeneratorConfig::from_env().ok(),
        prometheus_config: PrometheusConfig::from_env().ok(),
        proof_data_handler_config: ProofDataHandlerConfig::from_env().ok(),
        api_config: ApiConfig::from_env().ok(),
        db_config: DBConfig::from_env().ok(),
        eth_sender_config: EthConfig::from_env().ok(),
        eth_watch_config: EthWatchConfig::from_env().ok(),
        gas_adjuster_config: GasAdjusterConfig::from_env().ok(),
        object_store_config: ObjectStoreConfig::from_env().ok(),
        observability: ObservabilityConfig::from_env().ok(),
        snapshot_creator: SnapshotsCreatorConfig::from_env().ok(),
    })
}
