#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::{net::Ipv4Addr, str::FromStr, sync::Arc, time::Instant};

use anyhow::Context as _;
use futures::channel::oneshot;
use prometheus_exporter::PrometheusExporterConfig;
use temp_config_store::TempConfigStore;
use tokio::{sync::watch, task::JoinHandle};

use zksync_circuit_breaker::{
    l1_txs::FailedL1TransactionChecker, replication_lag::ReplicationLagChecker, CircuitBreaker,
    CircuitBreakerChecker, CircuitBreakerError,
};
use zksync_config::{
    configs::{
        api::{MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        contracts::ProverAtGenesis,
        database::MerkleTreeMode,
    },
    ApiConfig, ContractsConfig, DBConfig, ETHSenderConfig, PostgresConfig,
};
use zksync_contracts::{governance_contract, BaseSystemContracts};
use zksync_dal::{healthcheck::ConnectionPoolHealthCheck, ConnectionPool};
use zksync_eth_client::clients::http::QueryClient;
use zksync_eth_client::EthInterface;
use zksync_eth_client::{clients::http::PKSigningClient, BoundEthInterface};
use zksync_health_check::{CheckHealth, HealthStatus, ReactiveHealthCheck};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_utils::periodic_job::PeriodicJob;
use zksync_queued_job_processor::JobProcessor;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    proofs::AggregationRound,
    protocol_version::{L1VerifierConfig, VerifierParams},
    system_contracts::get_system_smart_contracts,
    L2ChainId, PackedEthSignature, ProtocolVersionId,
};
use zksync_verification_key_server::get_cached_commitments;

pub mod api_server;
pub mod basic_witness_input_producer;
pub mod block_reverter;
pub mod consistency_checker;
pub mod data_fetchers;
pub mod eth_sender;
pub mod eth_watch;
pub mod gas_tracker;
pub mod genesis;
pub mod house_keeper;
pub mod l1_gas_price;
pub mod metadata_calculator;
mod metrics;
pub mod proof_data_handler;
pub mod reorg_detector;
pub mod state_keeper;
pub mod sync_layer;
pub mod temp_config_store;
pub mod witness_generator;

use crate::api_server::healthcheck::HealthCheckHandle;
use crate::api_server::tx_sender::{TxSender, TxSenderBuilder, TxSenderConfig};
use crate::api_server::web3::{state::InternalApiConfig, ApiServerHandles, Namespace};
use crate::basic_witness_input_producer::BasicWitnessInputProducer;
use crate::eth_sender::{Aggregator, EthTxManager};
use crate::house_keeper::fri_proof_compressor_job_retry_manager::FriProofCompressorJobRetryManager;
use crate::house_keeper::fri_proof_compressor_queue_monitor::FriProofCompressorStatsReporter;
use crate::house_keeper::fri_prover_job_retry_manager::FriProverJobRetryManager;
use crate::house_keeper::fri_prover_queue_monitor::FriProverStatsReporter;
use crate::house_keeper::fri_scheduler_circuit_queuer::SchedulerCircuitQueuer;
use crate::house_keeper::fri_witness_generator_jobs_retry_manager::FriWitnessGeneratorJobRetryManager;
use crate::house_keeper::fri_witness_generator_queue_monitor::FriWitnessGeneratorStatsReporter;
use crate::house_keeper::{
    blocks_state_reporter::L1BatchMetricsReporter, gpu_prover_queue_monitor::GpuProverQueueMonitor,
    prover_job_retry_manager::ProverJobRetryManager, prover_queue_monitor::ProverStatsReporter,
    waiting_to_queued_fri_witness_job_mover::WaitingToQueuedFriWitnessJobMover,
    waiting_to_queued_witness_job_mover::WaitingToQueuedWitnessJobMover,
    witness_generator_queue_monitor::WitnessGeneratorStatsReporter,
};
use crate::l1_gas_price::{GasAdjusterSingleton, L1GasPriceProvider};
use crate::metadata_calculator::{
    MetadataCalculator, MetadataCalculatorConfig, MetadataCalculatorModeConfig,
};
use crate::state_keeper::{create_state_keeper, MempoolFetcher, MempoolGuard, MiniblockSealer};
use crate::witness_generator::{
    basic_circuits::BasicWitnessGenerator, leaf_aggregation::LeafAggregationWitnessGenerator,
    node_aggregation::NodeAggregationWitnessGenerator, scheduler::SchedulerWitnessGenerator,
};
use crate::{
    api_server::{
        contract_verification,
        execution_sandbox::{VmConcurrencyBarrier, VmConcurrencyLimiter},
        tx_sender::ApiContracts,
        web3,
    },
    data_fetchers::run_data_fetchers,
    eth_sender::EthTxAggregator,
    eth_watch::start_eth_watch,
    metrics::{InitStage, APP_METRICS},
};

/// Inserts the initial information about zkSync tokens into the database.
pub async fn genesis_init(
    postgres_config: &PostgresConfig,
    eth_sender: &ETHSenderConfig,
    network_config: &NetworkConfig,
    contracts_config: &ContractsConfig,
    eth_client_url: &str,
) -> anyhow::Result<()> {
    let db_url = postgres_config.master_url()?;
    let pool = ConnectionPool::singleton(db_url)
        .build()
        .await
        .context("failed to build connection_pool")?;
    let mut storage = pool.access_storage().await.context("access_storage()")?;
    let operator_address = PackedEthSignature::address_from_private_key(
        &eth_sender
            .sender
            .private_key()
            .context("Private key is required for genesis init")?,
    )
    .context("Failed to restore operator address from private key")?;

    // Select the first prover to be used during genesis.
    // Later we can change provers using the system upgrades, but for genesis
    // we should select one using the environment config.
    let first_l1_verifier_config =
        if matches!(contracts_config.prover_at_genesis, ProverAtGenesis::Fri) {
            let l1_verifier_config = L1VerifierConfig {
                params: VerifierParams {
                    recursion_node_level_vk_hash: contracts_config.fri_recursion_node_level_vk_hash,
                    recursion_leaf_level_vk_hash: contracts_config.fri_recursion_leaf_level_vk_hash,
                    recursion_circuits_set_vks_hash: zksync_types::H256::zero(),
                },
                recursion_scheduler_level_vk_hash: contracts_config.snark_wrapper_vk_hash,
            };

            let eth_client = QueryClient::new(eth_client_url)?;
            let vk_hash: zksync_types::H256 = eth_client
                .call_contract_function(
                    "verificationKeyHash",
                    (),
                    None,
                    Default::default(),
                    None,
                    contracts_config.verifier_addr,
                    zksync_contracts::verifier_contract(),
                )
                .await?;

            assert_eq!(
                vk_hash, l1_verifier_config.recursion_scheduler_level_vk_hash,
                "L1 verifier key does not match the one in the config"
            );

            l1_verifier_config
        } else {
            L1VerifierConfig {
                params: VerifierParams {
                    recursion_node_level_vk_hash: contracts_config.recursion_node_level_vk_hash,
                    recursion_leaf_level_vk_hash: contracts_config.recursion_leaf_level_vk_hash,
                    recursion_circuits_set_vks_hash: contracts_config
                        .recursion_circuits_set_vks_hash,
                },
                recursion_scheduler_level_vk_hash: contracts_config
                    .recursion_scheduler_level_vk_hash,
            }
        };

    genesis::ensure_genesis_state(
        &mut storage,
        network_config.zksync_network_id,
        &genesis::GenesisParams {
            // We consider the operator to be the first validator for now.
            first_validator: operator_address,
            protocol_version: ProtocolVersionId::latest(),
            base_system_contracts: BaseSystemContracts::load_from_disk(),
            system_contracts: get_system_smart_contracts(),
            first_verifier_address: contracts_config.verifier_addr,
            first_l1_verifier_config,
        },
    )
    .await?;
    Ok(())
}

pub async fn is_genesis_needed(postgres_config: &PostgresConfig) -> bool {
    let db_url = postgres_config.master_url().unwrap();
    let pool = ConnectionPool::singleton(db_url)
        .build()
        .await
        .expect("failed to build connection_pool");
    let mut storage = pool.access_storage().await.expect("access_storage()");
    storage.blocks_dal().is_genesis_needed().await.unwrap()
}

/// Sets up an interrupt handler and returns a future that resolves once an interrupt signal
/// is received.
pub fn setup_sigint_handler() -> oneshot::Receiver<()> {
    let (sigint_sender, sigint_receiver) = oneshot::channel();
    let mut sigint_sender = Some(sigint_sender);
    ctrlc::set_handler(move || {
        if let Some(sigint_sender) = sigint_sender.take() {
            sigint_sender.send(()).ok();
            // ^ The send fails if `sigint_receiver` is dropped. We're OK with this,
            // since at this point the node should be stopping anyway, or is not interested
            // in listening to interrupt signals.
        }
    })
    .expect("Error setting Ctrl+C handler");

    sigint_receiver
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Component {
    /// Public Web3 API running on HTTP server.
    HttpApi,
    /// Public Web3 API running on HTTP/WebSocket server and redirect eth_getLogs to another method.
    ApiTranslator,
    /// Public Web3 API (including PubSub) running on WebSocket server.
    WsApi,
    /// REST API for contract verification.
    ContractVerificationApi,
    /// Metadata calculator.
    Tree,
    // TODO(BFT-273): Remove `TreeLightweight` component as obsolete
    TreeLightweight,
    TreeBackup,
    /// Merkle tree API.
    TreeApi,
    EthWatcher,
    /// Eth tx generator.
    EthTxAggregator,
    /// Manager for eth tx.
    EthTxManager,
    /// Data fetchers: list fetcher, volume fetcher, price fetcher.
    DataFetcher,
    /// State keeper.
    StateKeeper,
    /// Produces input for basic witness generator and uploads it as bin encoded file (blob) to GCS.
    /// The blob is later used as input for Basic Witness Generators.
    BasicWitnessInputProducer,
    /// Witness Generator. The first argument is a number of jobs to process. If None, runs indefinitely.
    /// The second argument is the type of the witness-generation performed
    WitnessGenerator(Option<usize>, AggregationRound),
    /// Component for housekeeping task such as cleaning blobs from GCS, reporting metrics etc.
    Housekeeper,
    /// Component for exposing API's to prover for providing proof generation data and accepting proofs.
    ProofDataHandler,
}

#[derive(Debug)]
pub struct Components(pub Vec<Component>);

impl FromStr for Components {
    type Err = String;

    fn from_str(s: &str) -> Result<Components, String> {
        match s {
            "api" => Ok(Components(vec![
                Component::HttpApi,
                Component::WsApi,
                Component::ContractVerificationApi,
            ])),
            "http_api" => Ok(Components(vec![Component::HttpApi])),
            "http_api_translator" => Ok(Components(vec![Component::ApiTranslator])),
            "ws_api" => Ok(Components(vec![Component::WsApi])),
            "contract_verification_api" => Ok(Components(vec![Component::ContractVerificationApi])),
            "tree" | "tree_new" => Ok(Components(vec![Component::Tree])),
            "tree_lightweight" | "tree_lightweight_new" => {
                Ok(Components(vec![Component::TreeLightweight]))
            }
            "tree_backup" => Ok(Components(vec![Component::TreeBackup])),
            "tree_api" => Ok(Components(vec![Component::TreeApi])),
            "data_fetcher" => Ok(Components(vec![Component::DataFetcher])),
            "state_keeper" => Ok(Components(vec![Component::StateKeeper])),
            "housekeeper" => Ok(Components(vec![Component::Housekeeper])),
            "basic_witness_input_producer" => {
                Ok(Components(vec![Component::BasicWitnessInputProducer]))
            }
            "witness_generator" => Ok(Components(vec![
                Component::WitnessGenerator(None, AggregationRound::BasicCircuits),
                Component::WitnessGenerator(None, AggregationRound::LeafAggregation),
                Component::WitnessGenerator(None, AggregationRound::NodeAggregation),
                Component::WitnessGenerator(None, AggregationRound::Scheduler),
            ])),
            "one_shot_witness_generator" => Ok(Components(vec![
                Component::WitnessGenerator(Some(1), AggregationRound::BasicCircuits),
                Component::WitnessGenerator(Some(1), AggregationRound::LeafAggregation),
                Component::WitnessGenerator(Some(1), AggregationRound::NodeAggregation),
                Component::WitnessGenerator(Some(1), AggregationRound::Scheduler),
            ])),
            "one_shot_basic_witness_generator" => {
                Ok(Components(vec![Component::WitnessGenerator(
                    Some(1),
                    AggregationRound::BasicCircuits,
                )]))
            }
            "one_shot_leaf_witness_generator" => Ok(Components(vec![Component::WitnessGenerator(
                Some(1),
                AggregationRound::LeafAggregation,
            )])),
            "one_shot_node_witness_generator" => Ok(Components(vec![Component::WitnessGenerator(
                Some(1),
                AggregationRound::NodeAggregation,
            )])),
            "one_shot_scheduler_witness_generator" => {
                Ok(Components(vec![Component::WitnessGenerator(
                    Some(1),
                    AggregationRound::Scheduler,
                )]))
            }
            "eth" => Ok(Components(vec![
                Component::EthWatcher,
                Component::EthTxAggregator,
                Component::EthTxManager,
            ])),
            "eth_watcher" => Ok(Components(vec![Component::EthWatcher])),
            "eth_tx_aggregator" => Ok(Components(vec![Component::EthTxAggregator])),
            "eth_tx_manager" => Ok(Components(vec![Component::EthTxManager])),
            "proof_data_handler" => Ok(Components(vec![Component::ProofDataHandler])),
            other => Err(format!("{} is not a valid component name", other)),
        }
    }
}

pub async fn initialize_components(
    configs: &TempConfigStore,
    components: Vec<Component>,
    use_prometheus_push_gateway: bool,
) -> anyhow::Result<(
    Vec<JoinHandle<anyhow::Result<()>>>,
    watch::Sender<bool>,
    oneshot::Receiver<CircuitBreakerError>,
    HealthCheckHandle,
)> {
    tracing::info!("Starting the components: {components:?}");

    let db_config = configs.db_config.clone().context("db_config")?;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;

    let statement_timeout = postgres_config.statement_timeout();
    let pool_size = postgres_config.max_connections()?;
    let connection_pool = ConnectionPool::builder(postgres_config.master_url()?, pool_size)
        .build()
        .await
        .context("failed to build connection_pool")?;
    let prover_connection_pool = ConnectionPool::builder(postgres_config.prover_url()?, pool_size)
        .build()
        .await
        .context("failed to build prover_connection_pool")?;
    let replica_connection_pool =
        ConnectionPool::builder(postgres_config.replica_url()?, pool_size)
            .set_statement_timeout(statement_timeout)
            .build()
            .await
            .context("failed to build replica_connection_pool")?;

    let mut healthchecks: Vec<Box<dyn CheckHealth>> = Vec::new();
    let contracts_config = configs
        .contracts_config
        .clone()
        .context("contracts_config")?;
    let eth_client_config = configs
        .eth_client_config
        .clone()
        .context("eth_client_config")?;
    let circuit_breaker_config = configs
        .circuit_breaker_config
        .clone()
        .context("circuit_breaker_config")?;

    let circuit_breaker_checker = CircuitBreakerChecker::new(
        circuit_breakers_for_components(&components, &postgres_config, &circuit_breaker_config)
            .await
            .context("circuit_breakers_for_components")?,
        &circuit_breaker_config,
    );
    circuit_breaker_checker.check().await.unwrap_or_else(|err| {
        panic!("Circuit breaker triggered: {}", err);
    });

    let query_client = QueryClient::new(&eth_client_config.web3_url).unwrap();
    let gas_adjuster_config = configs.gas_adjuster_config.context("gas_adjuster_config")?;
    let mut gas_adjuster =
        GasAdjusterSingleton::new(eth_client_config.web3_url.clone(), gas_adjuster_config);

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (cb_sender, cb_receiver) = oneshot::channel();

    // Prometheus exporter and circuit breaker checker should run for every component configuration.
    let prom_config = configs
        .prometheus_config
        .clone()
        .context("prometheus_config")?;
    let prom_config = if use_prometheus_push_gateway {
        PrometheusExporterConfig::push(prom_config.gateway_endpoint(), prom_config.push_interval())
    } else {
        PrometheusExporterConfig::pull(prom_config.listener_port)
    };

    let (prometheus_health_check, prometheus_health_updater) =
        ReactiveHealthCheck::new("prometheus_exporter");
    healthchecks.push(Box::new(prometheus_health_check));
    let prometheus_task = prom_config.run(stop_receiver.clone());
    let prometheus_task = tokio::spawn(async move {
        prometheus_health_updater.update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(prometheus_health_updater);
        res
    });

    let mut task_futures: Vec<JoinHandle<anyhow::Result<()>>> = vec![
        prometheus_task,
        tokio::spawn(circuit_breaker_checker.run(cb_sender, stop_receiver.clone())),
    ];

    if components.contains(&Component::WsApi)
        || components.contains(&Component::HttpApi)
        || components.contains(&Component::ContractVerificationApi)
        || components.contains(&Component::ApiTranslator)
    {
        let api_config = configs.api_config.clone().context("api_config")?;
        let state_keeper_config = configs
            .state_keeper_config
            .clone()
            .context("state_keeper_config")?;
        let network_config = configs.network_config.clone().context("network_config")?;
        let tx_sender_config = TxSenderConfig::new(
            &state_keeper_config,
            &api_config.web3_json_rpc,
            network_config.zksync_network_id,
        );
        let internal_api_config = InternalApiConfig::new(
            &network_config,
            &api_config.web3_json_rpc,
            &contracts_config,
        );

        // Lazily initialize storage caches only when they are needed (e.g., skip their initialization
        // if we only run the explorer APIs). This is required because the cache update task will
        // terminate immediately if storage caches are dropped, which will lead to the (unexpected)
        // program termination.
        let mut storage_caches = None;

        if components.contains(&Component::HttpApi) {
            storage_caches = Some(
                build_storage_caches(configs, &replica_connection_pool, &mut task_futures)
                    .context("build_storage_caches()")?,
            );

            let started_at = Instant::now();
            tracing::info!("Initializing HTTP API");
            let bounded_gas_adjuster = gas_adjuster
                .get_or_init_bounded()
                .await
                .context("gas_adjuster.get_or_init_bounded()")?;
            let server_handles = run_http_api(
                &postgres_config,
                &tx_sender_config,
                &state_keeper_config,
                &internal_api_config,
                &api_config,
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
                bounded_gas_adjuster.clone(),
                state_keeper_config.save_call_traces,
                components.contains(&Component::ApiTranslator),
                storage_caches.clone().unwrap(),
            )
            .await
            .context("run_http_api")?;

            task_futures.extend(server_handles.tasks);
            healthchecks.push(Box::new(server_handles.health_check));
            let elapsed = started_at.elapsed();
            APP_METRICS.init_latency[&InitStage::HttpApi].set(elapsed);
            tracing::info!(
                "Initialized HTTP API on {:?} in {elapsed:?}",
                server_handles.local_addr
            );
        }

        if components.contains(&Component::WsApi) {
            let storage_caches = match storage_caches {
                Some(storage_caches) => storage_caches,
                None => build_storage_caches(configs, &replica_connection_pool, &mut task_futures)
                    .context("build_storage_caches()")?,
            };

            let started_at = Instant::now();
            tracing::info!("initializing WS API");
            let bounded_gas_adjuster = gas_adjuster
                .get_or_init_bounded()
                .await
                .context("gas_adjuster.get_or_init_bounded()")?;
            let server_handles = run_ws_api(
                &postgres_config,
                &tx_sender_config,
                &state_keeper_config,
                &internal_api_config,
                &api_config,
                bounded_gas_adjuster.clone(),
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
                storage_caches,
                components.contains(&Component::ApiTranslator),
            )
            .await
            .context("run_ws_api")?;

            task_futures.extend(server_handles.tasks);
            healthchecks.push(Box::new(server_handles.health_check));
            let elapsed = started_at.elapsed();
            APP_METRICS.init_latency[&InitStage::WsApi].set(elapsed);
            tracing::info!(
                "initialized WS API on {:?} in {elapsed:?}",
                server_handles.local_addr
            );
        }

        if components.contains(&Component::ContractVerificationApi) {
            let started_at = Instant::now();
            tracing::info!("initializing contract verification REST API");
            task_futures.push(contract_verification::start_server_thread_detached(
                connection_pool.clone(),
                replica_connection_pool.clone(),
                api_config.contract_verification.clone(),
                stop_receiver.clone(),
            ));
            let elapsed = started_at.elapsed();
            APP_METRICS.init_latency[&InitStage::ContractVerificationApi].set(elapsed);
            tracing::info!("initialized contract verification REST API in {elapsed:?}");
        }
    }

    let object_store_config = configs
        .object_store_config
        .clone()
        .context("object_store_config")?;
    let store_factory = ObjectStoreFactory::new(object_store_config);

    if components.contains(&Component::StateKeeper) {
        let started_at = Instant::now();
        tracing::info!("initializing State Keeper");
        let bounded_gas_adjuster = gas_adjuster
            .get_or_init_bounded()
            .await
            .context("gas_adjuster.get_or_init_bounded()")?;
        add_state_keeper_to_task_futures(
            &mut task_futures,
            &postgres_config,
            &contracts_config,
            configs
                .state_keeper_config
                .clone()
                .context("state_keeper_config")?,
            &configs.network_config.clone().context("network_config")?,
            &db_config,
            &configs.mempool_config.clone().context("mempool_config")?,
            bounded_gas_adjuster,
            store_factory.create_store().await,
            stop_receiver.clone(),
        )
        .await
        .context("add_state_keeper_to_task_futures()")?;

        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::StateKeeper].set(elapsed);
        tracing::info!("initialized State Keeper in {elapsed:?}");
    }

    let main_zksync_contract_address = contracts_config.diamond_proxy_addr;
    if components.contains(&Component::EthWatcher) {
        let started_at = Instant::now();
        tracing::info!("initializing ETH-Watcher");
        let eth_watch_pool = ConnectionPool::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build eth_watch_pool")?;
        let governance = (governance_contract(), contracts_config.governance_addr);
        let eth_watch_config = configs
            .eth_watch_config
            .clone()
            .context("eth_watch_config")?;
        task_futures.push(
            start_eth_watch(
                eth_watch_config,
                eth_watch_pool,
                query_client.clone(),
                main_zksync_contract_address,
                governance,
                stop_receiver.clone(),
            )
            .await
            .context("start_eth_watch()")?,
        );
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::EthWatcher].set(elapsed);
        tracing::info!("initialized ETH-Watcher in {elapsed:?}");
    }

    if components.contains(&Component::EthTxAggregator) {
        let started_at = Instant::now();
        tracing::info!("initializing ETH-TxAggregator");
        let eth_sender_pool = ConnectionPool::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build eth_sender_pool")?;
        let eth_sender_prover_pool = ConnectionPool::singleton(postgres_config.prover_url()?)
            .build()
            .await
            .context("failed to build eth_sender_prover_pool")?;

        let eth_sender = configs
            .eth_sender_config
            .clone()
            .context("eth_sender_config")?;
        let eth_client =
            PKSigningClient::from_config(&eth_sender, &contracts_config, &eth_client_config);
        let nonce = eth_client.pending_nonce("eth_sender").await.unwrap();
        let eth_tx_aggregator_actor = EthTxAggregator::new(
            eth_sender.sender.clone(),
            Aggregator::new(
                eth_sender.sender.clone(),
                store_factory.create_store().await,
            ),
            contracts_config.validator_timelock_addr,
            contracts_config.l1_multicall3_addr,
            main_zksync_contract_address,
            nonce.as_u64(),
        );
        task_futures.push(tokio::spawn(eth_tx_aggregator_actor.run(
            eth_sender_pool,
            eth_sender_prover_pool,
            eth_client,
            stop_receiver.clone(),
        )));
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::EthTxAggregator].set(elapsed);
        tracing::info!("initialized ETH-TxAggregator in {elapsed:?}");
    }

    if components.contains(&Component::EthTxManager) {
        let started_at = Instant::now();
        tracing::info!("initializing ETH-TxManager");
        let eth_manager_pool = ConnectionPool::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build eth_manager_pool")?;
        let eth_sender = configs
            .eth_sender_config
            .clone()
            .context("eth_sender_config")?;
        let eth_client =
            PKSigningClient::from_config(&eth_sender, &contracts_config, &eth_client_config);
        let eth_tx_manager_actor = EthTxManager::new(
            eth_sender.sender,
            gas_adjuster
                .get_or_init()
                .await
                .context("gas_adjuster.get_or_init()")?,
            eth_client,
        );
        task_futures.extend([tokio::spawn(
            eth_tx_manager_actor.run(eth_manager_pool, stop_receiver.clone()),
        )]);
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::EthTxManager].set(elapsed);
        tracing::info!("initialized ETH-TxManager in {elapsed:?}");
    }

    if components.contains(&Component::DataFetcher) {
        let started_at = Instant::now();
        let fetcher_config = configs.fetcher_config.clone().context("fetcher_config")?;
        let eth_network = configs.network_config.clone().context("network_config")?;
        tracing::info!("initializing data fetchers");
        task_futures.extend(run_data_fetchers(
            &fetcher_config,
            eth_network.network,
            connection_pool.clone(),
            stop_receiver.clone(),
        ));
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::DataFetcher].set(elapsed);
        tracing::info!("initialized data fetchers in {elapsed:?}");
    }

    add_trees_to_task_futures(
        configs,
        &mut task_futures,
        &mut healthchecks,
        &components,
        &store_factory,
        stop_receiver.clone(),
    )
    .await
    .context("add_trees_to_task_futures()")?;
    add_witness_generator_to_task_futures(
        configs,
        &mut task_futures,
        &components,
        &connection_pool,
        &prover_connection_pool,
        &store_factory,
        &stop_receiver,
    )
    .await
    .context("add_witness_generator_to_task_futures()")?;

    if components.contains(&Component::BasicWitnessInputProducer) {
        let singleton_connection_pool = ConnectionPool::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build singleton connection_pool")?;
        let network_config = configs.network_config.clone().context("network_config")?;
        add_basic_witness_input_producer_to_task_futures(
            &mut task_futures,
            &singleton_connection_pool,
            &store_factory,
            network_config.zksync_network_id,
            stop_receiver.clone(),
        )
        .await
        .context("add_basic_witness_input_producer_to_task_futures()")?;
    }

    if components.contains(&Component::Housekeeper) {
        add_house_keeper_to_task_futures(configs, &mut task_futures)
            .await
            .context("add_house_keeper_to_task_futures()")?;
    }

    if components.contains(&Component::ProofDataHandler) {
        task_futures.push(tokio::spawn(proof_data_handler::run_server(
            configs
                .proof_data_handler_config
                .clone()
                .context("proof_data_handler_config")?,
            configs
                .contracts_config
                .clone()
                .context("contracts_config")?,
            store_factory.create_store().await,
            connection_pool.clone(),
            stop_receiver.clone(),
        )));
    }

    // Run healthcheck server for all components.
    healthchecks.push(Box::new(ConnectionPoolHealthCheck::new(
        replica_connection_pool,
    )));

    let healtcheck_api_config = configs
        .health_check_config
        .clone()
        .context("health_check_config")?;
    let health_check_handle =
        HealthCheckHandle::spawn_server(healtcheck_api_config.bind_addr(), healthchecks);

    if let Some(task) = gas_adjuster.run_if_initialized(stop_receiver.clone()) {
        task_futures.push(task);
    }
    Ok((task_futures, stop_sender, cb_receiver, health_check_handle))
}

#[allow(clippy::too_many_arguments)]
async fn add_state_keeper_to_task_futures<E: L1GasPriceProvider + Send + Sync + 'static>(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    postgres_config: &PostgresConfig,
    contracts_config: &ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    network_config: &NetworkConfig,
    db_config: &DBConfig,
    mempool_config: &MempoolConfig,
    gas_adjuster: Arc<E>,
    object_store: Box<dyn ObjectStore>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let fair_l2_gas_price = state_keeper_config.fair_l2_gas_price;
    let pool_builder = ConnectionPool::singleton(postgres_config.master_url()?);
    let state_keeper_pool = pool_builder
        .build()
        .await
        .context("failed to build state_keeper_pool")?;
    let next_priority_id = state_keeper_pool
        .access_storage()
        .await
        .unwrap()
        .transactions_dal()
        .next_priority_id()
        .await;
    let mempool = MempoolGuard::new(next_priority_id, mempool_config.capacity);
    mempool.register_metrics();

    let miniblock_sealer_pool = pool_builder
        .build()
        .await
        .context("failed to build miniblock_sealer_pool")?;
    let (miniblock_sealer, miniblock_sealer_handle) = MiniblockSealer::new(
        miniblock_sealer_pool,
        state_keeper_config.miniblock_seal_queue_capacity,
    );
    task_futures.push(tokio::spawn(miniblock_sealer.run()));

    let state_keeper = create_state_keeper(
        contracts_config,
        state_keeper_config,
        db_config,
        network_config,
        mempool_config,
        state_keeper_pool,
        mempool.clone(),
        gas_adjuster.clone(),
        miniblock_sealer_handle,
        object_store,
        stop_receiver.clone(),
    )
    .await;
    task_futures.push(tokio::spawn(state_keeper.run()));

    let mempool_fetcher_pool = pool_builder
        .build()
        .await
        .context("failed to build mempool_fetcher_pool")?;
    let mempool_fetcher = MempoolFetcher::new(mempool, gas_adjuster, mempool_config);
    let mempool_fetcher_handle = tokio::spawn(mempool_fetcher.run(
        mempool_fetcher_pool,
        mempool_config.remove_stuck_txs,
        mempool_config.stuck_tx_timeout(),
        fair_l2_gas_price,
        stop_receiver,
    ));
    task_futures.push(mempool_fetcher_handle);
    Ok(())
}

async fn add_trees_to_task_futures(
    configs: &TempConfigStore,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    healthchecks: &mut Vec<Box<dyn CheckHealth>>,
    components: &[Component],
    store_factory: &ObjectStoreFactory,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    if components.contains(&Component::TreeBackup) {
        anyhow::bail!("Tree backup mode is disabled");
    }

    let db_config = configs.db_config.clone().context("db_config")?;
    let operation_config = configs
        .operations_manager_config
        .clone()
        .context("operations_manager_config")?;
    let api_config = configs
        .api_config
        .clone()
        .context("api_config")?
        .merkle_tree;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;
    let api_config = components
        .contains(&Component::TreeApi)
        .then_some(&api_config);

    let has_tree_component = components.contains(&Component::Tree);
    let has_lightweight_component = components.contains(&Component::TreeLightweight);
    let mode = match (has_tree_component, has_lightweight_component) {
        (true, true) => anyhow::bail!(
            "Cannot start a node with a Merkle tree in both full and lightweight modes. \
             Since the storage layout is mode-independent, choose either of modes and run \
             the node with it."
        ),
        (false, true) => MetadataCalculatorModeConfig::Lightweight,
        (true, false) => match db_config.merkle_tree.mode {
            MerkleTreeMode::Lightweight => MetadataCalculatorModeConfig::Lightweight,
            MerkleTreeMode::Full => MetadataCalculatorModeConfig::Full {
                store_factory: Some(store_factory),
            },
        },
        (false, false) => {
            anyhow::ensure!(
                !components.contains(&Component::TreeApi),
                "Merkle tree API cannot be started without a tree component"
            );
            return Ok(());
        }
    };

    run_tree(
        task_futures,
        healthchecks,
        &postgres_config,
        &db_config,
        api_config,
        &operation_config,
        mode,
        stop_receiver,
    )
    .await
    .context("run_tree()")
}

#[allow(clippy::too_many_arguments)]
async fn run_tree(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    healthchecks: &mut Vec<Box<dyn CheckHealth>>,
    postgres_config: &PostgresConfig,
    db_config: &DBConfig,
    api_config: Option<&MerkleTreeApiConfig>,
    operation_manager: &OperationsManagerConfig,
    mode: MetadataCalculatorModeConfig<'_>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let started_at = Instant::now();
    let mode_str = if matches!(mode, MetadataCalculatorModeConfig::Full { .. }) {
        "full"
    } else {
        "lightweight"
    };
    tracing::info!("Initializing Merkle tree in {mode_str} mode");

    let config =
        MetadataCalculatorConfig::for_main_node(&db_config.merkle_tree, operation_manager, mode);
    let metadata_calculator = MetadataCalculator::new(&config).await;
    if let Some(api_config) = api_config {
        let address = (Ipv4Addr::UNSPECIFIED, api_config.port).into();
        let server_task = metadata_calculator
            .tree_reader()
            .run_api_server(address, stop_receiver.clone());
        task_futures.push(tokio::spawn(server_task));
    }

    let tree_health_check = metadata_calculator.tree_health_check();
    healthchecks.push(Box::new(tree_health_check));
    let pool = ConnectionPool::singleton(postgres_config.master_url()?)
        .build()
        .await
        .context("failed to build connection pool")?;
    let prover_pool = ConnectionPool::singleton(postgres_config.prover_url()?)
        .build()
        .await
        .context("failed to build prover_pool")?;
    let tree_task = tokio::spawn(metadata_calculator.run(pool, prover_pool, stop_receiver));
    task_futures.push(tree_task);

    let elapsed = started_at.elapsed();
    APP_METRICS.init_latency[&InitStage::Tree].set(elapsed);
    tracing::info!("Initialized {mode_str} tree in {elapsed:?}");
    Ok(())
}

async fn add_basic_witness_input_producer_to_task_futures(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    connection_pool: &ConnectionPool,
    store_factory: &ObjectStoreFactory,
    l2_chain_id: L2ChainId,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    // Witness Generator won't be spawned with `ZKSYNC_LOCAL_SETUP` running.
    // BasicWitnessInputProducer shouldn't be producing input for it locally either.
    if std::env::var("ZKSYNC_LOCAL_SETUP") == Ok("true".to_owned()) {
        return Ok(());
    }
    let started_at = Instant::now();
    tracing::info!("initializing BasicWitnessInputProducer");
    let producer =
        BasicWitnessInputProducer::new(connection_pool.clone(), store_factory, l2_chain_id).await?;
    task_futures.push(tokio::spawn(producer.run(stop_receiver, None)));
    tracing::info!(
        "Initialized BasicWitnessInputProducer in {:?}",
        started_at.elapsed()
    );
    let elapsed = started_at.elapsed();
    APP_METRICS.init_latency[&InitStage::BasicWitnessInputProducer].set(elapsed);
    Ok(())
}

async fn add_witness_generator_to_task_futures(
    configs: &TempConfigStore,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    components: &[Component],
    connection_pool: &ConnectionPool,
    prover_connection_pool: &ConnectionPool,
    store_factory: &ObjectStoreFactory,
    stop_receiver: &watch::Receiver<bool>,
) -> anyhow::Result<()> {
    // We don't want witness generator to run on local nodes, as it's CPU heavy.
    if std::env::var("ZKSYNC_LOCAL_SETUP") == Ok("true".to_owned()) {
        return Ok(());
    }

    let generator_params = components.iter().filter_map(|component| {
        if let Component::WitnessGenerator(batch_size, component_type) = component {
            Some((*batch_size, *component_type))
        } else {
            None
        }
    });

    for (batch_size, component_type) in generator_params {
        let started_at = Instant::now();
        tracing::info!(
            "initializing the {component_type:?} witness generator, batch size: {batch_size:?}"
        );

        let vk_commitments = get_cached_commitments();
        let protocol_versions = prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .protocol_versions_dal()
            .protocol_version_for(&vk_commitments)
            .await;
        let config = configs
            .witness_generator_config
            .clone()
            .context("witness_generator_config")?;
        let task = match component_type {
            AggregationRound::BasicCircuits => {
                let witness_generator = BasicWitnessGenerator::new(
                    config,
                    store_factory,
                    protocol_versions.clone(),
                    connection_pool.clone(),
                    prover_connection_pool.clone(),
                )
                .await;
                tokio::spawn(witness_generator.run(stop_receiver.clone(), batch_size))
            }
            AggregationRound::LeafAggregation => {
                let witness_generator = LeafAggregationWitnessGenerator::new(
                    config,
                    store_factory,
                    protocol_versions.clone(),
                    connection_pool.clone(),
                    prover_connection_pool.clone(),
                )
                .await;
                tokio::spawn(witness_generator.run(stop_receiver.clone(), batch_size))
            }
            AggregationRound::NodeAggregation => {
                let witness_generator = NodeAggregationWitnessGenerator::new(
                    config,
                    store_factory,
                    protocol_versions.clone(),
                    connection_pool.clone(),
                    prover_connection_pool.clone(),
                )
                .await;
                tokio::spawn(witness_generator.run(stop_receiver.clone(), batch_size))
            }
            AggregationRound::Scheduler => {
                let witness_generator = SchedulerWitnessGenerator::new(
                    config,
                    store_factory,
                    protocol_versions.clone(),
                    connection_pool.clone(),
                    prover_connection_pool.clone(),
                )
                .await;
                tokio::spawn(witness_generator.run(stop_receiver.clone(), batch_size))
            }
        };
        task_futures.push(task);

        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::WitnessGenerator(component_type)].set(elapsed);
        tracing::info!("initialized {component_type:?} witness generator in {elapsed:?}");
    }
    Ok(())
}

async fn add_house_keeper_to_task_futures(
    configs: &TempConfigStore,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
) -> anyhow::Result<()> {
    let house_keeper_config = configs
        .house_keeper_config
        .clone()
        .context("house_keeper_config")?;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;
    let connection_pool = ConnectionPool::singleton(postgres_config.replica_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
        house_keeper_config.l1_batch_metrics_reporting_interval_ms,
        connection_pool,
    );

    let prover_connection_pool = ConnectionPool::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a prover_connection_pool")?;
    let prover_group_config = configs
        .prover_group_config
        .clone()
        .context("prover_group_config")?;
    let prover_configs = configs.prover_configs.clone().context("prover_configs")?;
    let gpu_prover_queue = GpuProverQueueMonitor::new(
        prover_group_config.synthesizer_per_gpu,
        house_keeper_config.gpu_prover_queue_reporting_interval_ms,
        prover_connection_pool.clone(),
    );
    let config = prover_configs.non_gpu.clone();
    let prover_job_retry_manager = ProverJobRetryManager::new(
        config.max_attempts,
        config.proof_generation_timeout(),
        house_keeper_config.prover_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    let prover_stats_reporter = ProverStatsReporter::new(
        house_keeper_config.prover_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
        prover_group_config.clone(),
    );
    let waiting_to_queued_witness_job_mover = WaitingToQueuedWitnessJobMover::new(
        house_keeper_config.witness_job_moving_interval_ms,
        prover_connection_pool.clone(),
    );
    let witness_generator_stats_reporter = WitnessGeneratorStatsReporter::new(
        house_keeper_config.witness_generator_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
    );

    task_futures.push(tokio::spawn(witness_generator_stats_reporter.run()));
    task_futures.push(tokio::spawn(gpu_prover_queue.run()));
    task_futures.push(tokio::spawn(l1_batch_metrics_reporter.run()));
    task_futures.push(tokio::spawn(prover_stats_reporter.run()));
    task_futures.push(tokio::spawn(waiting_to_queued_witness_job_mover.run()));
    task_futures.push(tokio::spawn(prover_job_retry_manager.run()));

    // All FRI Prover related components are configured below.
    let fri_prover_config = configs
        .fri_prover_config
        .clone()
        .context("fri_prover_config")?;
    let fri_prover_job_retry_manager = FriProverJobRetryManager::new(
        fri_prover_config.max_attempts,
        fri_prover_config.proof_generation_timeout(),
        house_keeper_config.fri_prover_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(fri_prover_job_retry_manager.run()));

    let fri_witness_gen_config = configs
        .fri_witness_generator_config
        .clone()
        .context("fri_witness_generator_config")?;
    let fri_witness_gen_job_retry_manager = FriWitnessGeneratorJobRetryManager::new(
        fri_witness_gen_config.max_attempts,
        fri_witness_gen_config.witness_generation_timeout(),
        house_keeper_config.fri_witness_generator_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(fri_witness_gen_job_retry_manager.run()));

    let waiting_to_queued_fri_witness_job_mover = WaitingToQueuedFriWitnessJobMover::new(
        house_keeper_config.fri_witness_job_moving_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(waiting_to_queued_fri_witness_job_mover.run()));

    let scheduler_circuit_queuer = SchedulerCircuitQueuer::new(
        house_keeper_config.fri_witness_job_moving_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(scheduler_circuit_queuer.run()));

    let fri_witness_generator_stats_reporter = FriWitnessGeneratorStatsReporter::new(
        prover_connection_pool.clone(),
        house_keeper_config.witness_generator_stats_reporting_interval_ms,
    );
    task_futures.push(tokio::spawn(fri_witness_generator_stats_reporter.run()));

    let fri_prover_stats_reporter = FriProverStatsReporter::new(
        house_keeper_config.fri_prover_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(fri_prover_stats_reporter.run()));

    let proof_compressor_config = configs
        .fri_proof_compressor_config
        .clone()
        .context("fri_proof_compressor_config")?;
    let fri_proof_compressor_stats_reporter = FriProofCompressorStatsReporter::new(
        house_keeper_config.fri_proof_compressor_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(fri_proof_compressor_stats_reporter.run()));

    let fri_proof_compressor_retry_manager = FriProofCompressorJobRetryManager::new(
        proof_compressor_config.max_attempts,
        proof_compressor_config.generation_timeout(),
        house_keeper_config.fri_proof_compressor_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    task_futures.push(tokio::spawn(fri_proof_compressor_retry_manager.run()));
    Ok(())
}

fn build_storage_caches(
    configs: &TempConfigStore,
    replica_connection_pool: &ConnectionPool,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
) -> anyhow::Result<PostgresStorageCaches> {
    let rpc_config = configs
        .web3_json_rpc_config
        .clone()
        .context("web3_json_rpc_config")?;
    let factory_deps_capacity = rpc_config.factory_deps_cache_size() as u64;
    let initial_writes_capacity = rpc_config.initial_writes_cache_size() as u64;
    let values_capacity = rpc_config.latest_values_cache_size() as u64;
    let mut storage_caches =
        PostgresStorageCaches::new(factory_deps_capacity, initial_writes_capacity);

    if values_capacity > 0 {
        let values_cache_task = storage_caches.configure_storage_values_cache(
            values_capacity,
            replica_connection_pool.clone(),
            tokio::runtime::Handle::current(),
        );
        task_futures.push(tokio::task::spawn_blocking(values_cache_task));
    }
    Ok(storage_caches)
}

async fn build_tx_sender<G: L1GasPriceProvider>(
    tx_sender_config: &TxSenderConfig,
    web3_json_config: &Web3JsonRpcConfig,
    state_keeper_config: &StateKeeperConfig,
    replica_pool: ConnectionPool,
    master_pool: ConnectionPool,
    l1_gas_price_provider: Arc<G>,
    storage_caches: PostgresStorageCaches,
) -> (TxSender<G>, VmConcurrencyBarrier) {
    let mut tx_sender_builder = TxSenderBuilder::new(tx_sender_config.clone(), replica_pool)
        .with_main_connection_pool(master_pool)
        .with_state_keeper_config(state_keeper_config.clone());

    // Add rate limiter if enabled.
    if let Some(transactions_per_sec_limit) = web3_json_config.transactions_per_sec_limit {
        tx_sender_builder = tx_sender_builder.with_rate_limiter(transactions_per_sec_limit);
    };

    let max_concurrency = web3_json_config.vm_concurrency_limit();
    let (vm_concurrency_limiter, vm_barrier) = VmConcurrencyLimiter::new(max_concurrency);

    let tx_sender = tx_sender_builder
        .build(
            l1_gas_price_provider,
            Arc::new(vm_concurrency_limiter),
            ApiContracts::load_from_disk(),
            storage_caches,
        )
        .await;
    (tx_sender, vm_barrier)
}

#[allow(clippy::too_many_arguments)]
async fn run_http_api<G: L1GasPriceProvider + Send + Sync + 'static>(
    postgres_config: &PostgresConfig,
    tx_sender_config: &TxSenderConfig,
    state_keeper_config: &StateKeeperConfig,
    internal_api: &InternalApiConfig,
    api_config: &ApiConfig,
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    gas_adjuster: Arc<G>,
    with_debug_namespace: bool,
    with_logs_request_translator_enabled: bool,
    storage_caches: PostgresStorageCaches,
) -> anyhow::Result<ApiServerHandles> {
    let (tx_sender, vm_barrier) = build_tx_sender(
        tx_sender_config,
        &api_config.web3_json_rpc,
        state_keeper_config,
        replica_connection_pool.clone(),
        master_connection_pool,
        gas_adjuster,
        storage_caches,
    )
    .await;

    let namespaces = if with_debug_namespace {
        Namespace::ALL.to_vec()
    } else {
        Namespace::NON_DEBUG.to_vec()
    };
    let last_miniblock_pool = ConnectionPool::singleton(postgres_config.replica_url()?)
        .build()
        .await
        .context("failed to build last_miniblock_pool")?;

    let mut api_builder =
        web3::ApiBuilder::jsonrpsee_backend(internal_api.clone(), replica_connection_pool)
            .http(api_config.web3_json_rpc.http_port)
            .with_last_miniblock_pool(last_miniblock_pool)
            .with_filter_limit(api_config.web3_json_rpc.filters_limit())
            .with_threads(api_config.web3_json_rpc.http_server_threads())
            .with_batch_request_size_limit(api_config.web3_json_rpc.max_batch_request_size())
            .with_response_body_size_limit(api_config.web3_json_rpc.max_response_body_size())
            .with_tx_sender(tx_sender, vm_barrier)
            .enable_api_namespaces(namespaces);
    if with_logs_request_translator_enabled {
        api_builder = api_builder.enable_request_translator();
    }
    api_builder.build(stop_receiver).await
}

#[allow(clippy::too_many_arguments)]
async fn run_ws_api<G: L1GasPriceProvider + Send + Sync + 'static>(
    postgres_config: &PostgresConfig,
    tx_sender_config: &TxSenderConfig,
    state_keeper_config: &StateKeeperConfig,
    internal_api: &InternalApiConfig,
    api_config: &ApiConfig,
    gas_adjuster: Arc<G>,
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    storage_caches: PostgresStorageCaches,
    with_logs_request_translator_enabled: bool,
) -> anyhow::Result<ApiServerHandles> {
    let (tx_sender, vm_barrier) = build_tx_sender(
        tx_sender_config,
        &api_config.web3_json_rpc,
        state_keeper_config,
        replica_connection_pool.clone(),
        master_connection_pool,
        gas_adjuster,
        storage_caches,
    )
    .await;
    let last_miniblock_pool = ConnectionPool::singleton(postgres_config.replica_url()?)
        .build()
        .await
        .context("failed to build last_miniblock_pool")?;

    let mut api_builder =
        web3::ApiBuilder::jsonrpc_backend(internal_api.clone(), replica_connection_pool)
            .ws(api_config.web3_json_rpc.ws_port)
            .with_last_miniblock_pool(last_miniblock_pool)
            .with_filter_limit(api_config.web3_json_rpc.filters_limit())
            .with_subscriptions_limit(api_config.web3_json_rpc.subscriptions_limit())
            .with_batch_request_size_limit(api_config.web3_json_rpc.max_batch_request_size())
            .with_response_body_size_limit(api_config.web3_json_rpc.max_response_body_size())
            .with_websocket_requests_per_minute_limit(
                api_config
                    .web3_json_rpc
                    .websocket_requests_per_minute_limit(),
            )
            .with_polling_interval(api_config.web3_json_rpc.pubsub_interval())
            .with_threads(api_config.web3_json_rpc.ws_server_threads())
            .with_tx_sender(tx_sender, vm_barrier)
            .enable_api_namespaces(Namespace::NON_DEBUG.to_vec());

    if with_logs_request_translator_enabled {
        api_builder = api_builder.enable_request_translator();
    }
    api_builder.build(stop_receiver.clone()).await
}

async fn circuit_breakers_for_components(
    components: &[Component],
    postgres_config: &PostgresConfig,
    circuit_breaker_config: &CircuitBreakerConfig,
) -> anyhow::Result<Vec<Box<dyn CircuitBreaker>>> {
    let mut circuit_breakers: Vec<Box<dyn CircuitBreaker>> = Vec::new();

    if components.iter().any(|c| {
        matches!(
            c,
            Component::EthTxAggregator | Component::EthTxManager | Component::StateKeeper
        )
    }) {
        let pool = ConnectionPool::singleton(postgres_config.replica_url()?)
            .build()
            .await
            .context("failed to build a connection pool")?;
        circuit_breakers.push(Box::new(FailedL1TransactionChecker { pool }));
    }

    if components.iter().any(|c| {
        matches!(
            c,
            Component::HttpApi
                | Component::WsApi
                | Component::ApiTranslator
                | Component::ContractVerificationApi
        )
    }) {
        let pool = ConnectionPool::singleton(postgres_config.replica_url()?)
            .build()
            .await?;
        circuit_breakers.push(Box::new(ReplicationLagChecker {
            pool,
            replication_lag_limit_sec: circuit_breaker_config.replication_lag_limit_sec,
        }));
    }
    Ok(circuit_breakers)
}
