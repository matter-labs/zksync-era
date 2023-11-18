use anyhow::Context as _;
use clap::Parser;

use std::{str::FromStr, time::Duration};

use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig, PrometheusConfig,
        ProofDataHandlerConfig, ProverGroupConfig, WitnessGeneratorConfig,
    },
    ApiConfig, ContractsConfig, DBConfig, ETHClientConfig, ETHSenderConfig, ETHWatchConfig,
    FetcherConfig, GasAdjusterConfig, ObjectStoreConfig, PostgresConfig, ProverConfigs,
};

use zksync_core::temp_config_store::TempConfigStore;
use zksync_core::{
    genesis_init, initialize_components, is_genesis_needed, setup_sigint_handler, Component,
    Components,
};
use zksync_env_config::FromEnv;
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::wait_for_tasks;

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
        default_value = "api,tree,eth,data_fetcher,state_keeper,witness_generator,housekeeper,basic_witness_input_producer"
    )]
    components: ComponentsToRun,
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

    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
    }

    // TODO (QIT-22): Only deserialize configs on demand.
    // Right now, we are trying to deserialize all the configs that may be needed by `zksync_core`.
    // "May" is the key word here, since some configs are only used by certain component configuration,
    // hence we are using `Option`s.
    let configs: TempConfigStore = TempConfigStore {
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
        fri_prover_config: FriProverConfig::from_env().ok(),
        fri_prover_group_config: FriProverGroupConfig::from_env().ok(),
        fri_witness_generator_config: FriWitnessGeneratorConfig::from_env().ok(),
        prometheus_config: PrometheusConfig::from_env().ok(),
        proof_data_handler_config: ProofDataHandlerConfig::from_env().ok(),
        prover_group_config: ProverGroupConfig::from_env().ok(),
        witness_generator_config: WitnessGeneratorConfig::from_env().ok(),
        api_config: ApiConfig::from_env().ok(),
        contracts_config: ContractsConfig::from_env().ok(),
        db_config: DBConfig::from_env().ok(),
        eth_client_config: ETHClientConfig::from_env().ok(),
        eth_sender_config: ETHSenderConfig::from_env().ok(),
        eth_watch_config: ETHWatchConfig::from_env().ok(),
        fetcher_config: FetcherConfig::from_env().ok(),
        gas_adjuster_config: GasAdjusterConfig::from_env().ok(),
        prover_configs: ProverConfigs::from_env().ok(),
        object_store_config: ObjectStoreConfig::from_env().ok(),
    };

    let postgres_config = configs.postgres_config.clone().context("PostgresConfig")?;

    if opt.genesis || is_genesis_needed(&postgres_config).await {
        let network = NetworkConfig::from_env().context("NetworkConfig")?;
        let eth_sender = ETHSenderConfig::from_env().context("ETHSenderConfig")?;
        let contracts = ContractsConfig::from_env().context("ContractsConfig")?;
        let eth_client = ETHClientConfig::from_env().context("EthClientConfig")?;
        genesis_init(
            &postgres_config,
            &eth_sender,
            &network,
            &contracts,
            &eth_client.web3_url,
        )
        .await
        .context("genesis_init")?;
        if opt.genesis {
            return Ok(());
        }
    }

    let components = if opt.rebuild_tree {
        vec![Component::Tree]
    } else {
        opt.components.0
    };

    // OneShotWitnessGenerator is the only component that is not expected to run indefinitely
    // if this value is `false`, we expect all components to run indefinitely: we panic if any component returns.
    let is_only_oneshot_witness_generator_task = matches!(
        components.as_slice(),
        [Component::WitnessGenerator(Some(_), _)]
    );

    // Run core actors.
    let (core_task_handles, stop_sender, cb_receiver, health_check_handle) =
        initialize_components(&configs, components, is_only_oneshot_witness_generator_task)
            .await
            .context("Unable to start Core actors")?;

    tracing::info!("Running {} core task handlers", core_task_handles.len());
    let sigint_receiver = setup_sigint_handler();

    let particular_crypto_alerts = None::<Vec<String>>;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = is_only_oneshot_witness_generator_task;
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
