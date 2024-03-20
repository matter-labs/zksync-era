use std::{path::PathBuf, str::FromStr, time::Duration};

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
        ContractsConfigReduced, FriProofCompressorConfig, FriProverConfig,
        FriWitnessGeneratorConfig, ObservabilityConfig, PrometheusConfig, ProofDataHandlerConfig,
        WitnessGeneratorConfig,
    },
    ApiConfig, ContractsConfig, DBConfig, ETHClientConfig, ETHSenderConfig, ETHWatchConfig,
    GasAdjusterConfig, GenesisConfig, ObjectStoreConfig, PostgresConfig,
};
use zksync_core::{
    genesis, genesis_init, initialize_components, is_genesis_needed, setup_sigint_handler,
    temp_config_store::{decode_yaml, decode_yaml_repr, Secrets, TempConfigStore},
    Component, Components,
};
use zksync_env_config::FromEnv;
use zksync_protobuf_config;
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::wait_for_tasks;

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
    /// Set chain id (temporary will be moved to genesis config)
    #[arg(long)]
    set_chain_id: bool,
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
    /// Path to the yaml with secrets. If set, it will be used instead of env vars.
    #[arg(long)]
    contracts_config_path: Option<std::path::PathBuf>,
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

    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
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

    // TODO (QIT-22): Only deserialize configs on demand.
    // Right now, we are trying to deserialize all the configs that may be needed by `zksync_core`.
    // "May" is the key word here, since some configs are only used by certain component configuration,
    // hence we are using `Option`s.
    let configs: TempConfigStore = match opt.config_path {
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml(&yaml).context("failed decoding YAML config")?
        }
        None => TempConfigStore {
            postgres_config: PostgresConfig::from_env().ok(),
            health_check_config: HealthCheckConfig::from_env().ok(),
            merkle_tree_api_config: MerkleTreeApiConfig::from_env().ok(),
            web3_json_rpc_config: Web3JsonRpcConfig::from_env().ok(),
            circuit_breaker_config: CircuitBreakerConfig::from_env().ok(),
            mempool_config: MempoolConfig::from_env().ok(),
            network_config: NetworkConfig::from_env().ok(),
            operations_manager_config: OperationsManagerConfig::from_env().ok(),
            state_keeper_config: StateKeeperConfig::from_env().ok(),
            house_keeper_config: HouseKeeperConfig::from_env().ok(),
            fri_proof_compressor_config: FriProofCompressorConfig::from_env().ok(),
            fri_prover_config: Some(FriProverConfig::from_env().context("fri_prover_config")?),
            fri_prover_group_config: FriProverGroupConfig::from_env().ok(),
            fri_witness_generator_config: FriWitnessGeneratorConfig::from_env().ok(),
            prometheus_config: PrometheusConfig::from_env().ok(),
            proof_data_handler_config: ProofDataHandlerConfig::from_env().ok(),
            witness_generator_config: WitnessGeneratorConfig::from_env().ok(),
            api_config: ApiConfig::from_env().ok(),
            db_config: DBConfig::from_env().ok(),
            eth_client_config: ETHClientConfig::from_env().ok(),
            eth_sender_config: ETHSenderConfig::from_env().ok(),
            eth_watch_config: ETHWatchConfig::from_env().ok(),
            gas_adjuster_config: GasAdjusterConfig::from_env().ok(),
            object_store_config: ObjectStoreConfig::from_env().ok(),
            consensus_config: config::read_consensus_config().context("read_consensus_config()")?,
        },
    };
    let secrets: Secrets = match opt.secrets_path {
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml(&yaml).context("failed decoding YAML config")?
        }
        None => Secrets {
            consensus: config::read_consensus_secrets().context("read_consensus_secrets()")?,
        },
    };

    let contracts_config: ContractsConfigReduced = match opt.contracts_config_path {
        None => ContractsConfig::from_env()
            .context("contracts_config")?
            .into(),
        Some(path) => {
            let yaml =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            decode_yaml_repr::<zksync_protobuf_config::proto::contracts::Contracts>(&yaml)
                .context("failed decoding YAML config")?
        }
    };
    // let contracts_config: ContractsConfigReduced = contracts_config.into();
    let postgres_config = configs.postgres_config.clone().context("PostgresConfig")?;

    if opt.genesis || is_genesis_needed(&postgres_config).await {
        let genesis = GenesisConfig::from_env().context("Genesis config")?;
        genesis_init(genesis, &postgres_config)
            .await
            .context("genesis_init")?;
        if opt.genesis {
            return Ok(());
        }
    }

    if opt.set_chain_id {
        let eth_client = ETHClientConfig::from_env().context("EthClientConfig")?;
        let genesis = GenesisConfig::from_env().context("Genesis config")?;

        if let Some(bridge) = genesis.shared_bridge {
            genesis::save_set_chain_id_tx(
                &eth_client.web3_url,
                contracts_config.diamond_proxy_addr,
                bridge.state_transition_proxy_addr,
                &postgres_config,
            )
            .await
            .context("Failed to save SetChainId upgrade transaction")?;
        }
    }

    let components = if opt.rebuild_tree {
        vec![Component::Tree]
    } else {
        opt.components.0
    };

    // Run core actors.
    let (core_task_handles, stop_sender, cb_receiver, health_check_handle) =
        initialize_components(&configs, &contracts_config, components, &secrets)
            .await
            .context("Unable to start Core actors")?;

    tracing::info!("Running {} core task handlers", core_task_handles.len());

    let particular_crypto_alerts = None::<Vec<String>>;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(core_task_handles, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = sigint_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
        error = cb_receiver => {
            if let Ok(error_msg) = error {
                let err = format!("Circuit breaker received, shutting down. Reason: {}", error_msg);
                tracing::warn!("{err}");
                vlog::capture_message(&err, vlog::AlertLevel::Warning);
            }
        },
    }

    stop_sender.send(true).ok();
    tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .unwrap();
    // Sleep for some time to let some components gracefully stop.
    tokio::time::sleep(Duration::from_secs(5)).await;
    health_check_handle.stop().await;
    tracing::info!("Stopped");
    Ok(())
}
