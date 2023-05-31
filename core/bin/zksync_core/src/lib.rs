#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::future::Future;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::channel::oneshot;
use futures::future;
use tokio::runtime::Builder;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use house_keeper::periodic_job::PeriodicJob;
use prometheus_exporter::run_prometheus_exporter;
use zksync_circuit_breaker::{
    facet_selectors::FacetSelectorsChecker, l1_txs::FailedL1TransactionChecker, vks::VksChecker,
    CircuitBreaker, CircuitBreakerChecker, CircuitBreakerError,
};

use zksync_config::configs::house_keeper::HouseKeeperConfig;
use zksync_config::configs::{ProverGroupConfig, WitnessGeneratorConfig};
use zksync_config::{ProverConfigs, ZkSyncConfig};
use zksync_dal::healthcheck::ConnectionPoolHealthCheck;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::clients::http::PKSigningClient;
use zksync_eth_client::BoundEthInterface;
use zksync_health_check::CheckHealth;
use zksync_mempool::MempoolStore;
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::AggregationRound;
use zksync_types::L2ChainId;

use crate::api_server::healthcheck;
use crate::api_server::tx_sender::{TxSender, TxSenderBuilder};
use crate::eth_sender::{Aggregator, EthTxManager};
use crate::fee_monitor::FeeMonitor;
use crate::house_keeper::blocks_state_reporter::L1BatchMetricsReporter;
use crate::house_keeper::gcs_blob_cleaner::GcsBlobCleaner;
use crate::house_keeper::gpu_prover_queue_monitor::GpuProverQueueMonitor;
use crate::house_keeper::{
    prover_job_retry_manager::ProverJobRetryManager, prover_queue_monitor::ProverStatsReporter,
    waiting_to_queued_witness_job_mover::WaitingToQueuedWitnessJobMover,
    witness_generator_queue_monitor::WitnessGeneratorStatsReporter,
};
use crate::l1_gas_price::BoundedGasAdjuster;
use crate::l1_gas_price::L1GasPriceProvider;
use crate::metadata_calculator::{MetadataCalculator, TreeHealthCheck, TreeImplementation};
use crate::state_keeper::mempool_actor::MempoolFetcher;
use crate::state_keeper::MempoolGuard;
use crate::witness_generator::basic_circuits::BasicWitnessGenerator;
use crate::witness_generator::leaf_aggregation::LeafAggregationWitnessGenerator;
use crate::witness_generator::node_aggregation::NodeAggregationWitnessGenerator;
use crate::witness_generator::scheduler::SchedulerWitnessGenerator;
use crate::{
    api_server::{explorer, web3},
    data_fetchers::run_data_fetchers,
    eth_sender::EthTxAggregator,
    eth_watch::start_eth_watch,
    l1_gas_price::GasAdjuster,
};

pub mod api_server;
pub mod block_reverter;
pub mod consistency_checker;
pub mod data_fetchers;
pub mod eth_sender;
pub mod eth_watch;
pub mod fee_monitor;
pub mod fee_ticker;
pub mod gas_tracker;
pub mod genesis;
pub mod house_keeper;
pub mod l1_gas_price;
pub mod metadata_calculator;
pub mod reorg_detector;
pub mod state_keeper;
pub mod sync_layer;
pub mod witness_generator;

/// Waits for *any* of the tokio tasks to be finished.
/// Since thesks are used as actors which should live as long
/// as application runs, any possible outcome (either `Ok` or `Err`) is considered
/// as a reason to stop the server completely.
pub async fn wait_for_tasks(task_futures: Vec<JoinHandle<()>>, tasks_allowed_to_finish: bool) {
    match future::select_all(task_futures).await.0 {
        Ok(_) => {
            if tasks_allowed_to_finish {
                vlog::info!("One of the actors finished its run. Finishing execution.");
            } else {
                vlog::error!(
                    "One of the actors finished its run, while it wasn't expected to do it"
                );
            }
        }
        Err(error) => {
            vlog::error!(
                "One of the tokio actors unexpectedly finished, shutting down: {:?}",
                error
            );
        }
    }
}

/// Inserts the initial information about zkSync tokens into the database.
pub async fn genesis_init(config: ZkSyncConfig) {
    let mut storage = StorageProcessor::establish_connection_blocking(true);
    genesis::ensure_genesis_state(
        &mut storage,
        L2ChainId(config.chain.eth.zksync_network_id),
        genesis::GenesisParams::MainNode {
            // We consider the operator to be the first validator for now.
            first_validator: config.eth_sender.sender.operator_commit_eth_addr,
        },
    )
    .await;
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
    // Public Web3 API running on HTTP server.
    HttpApi,
    // Public Web3 API (including PubSub) running on WebSocket server.
    WsApi,
    // REST API for explorer.
    ExplorerApi,
    // Metadata Calculator.
    Tree,
    TreeNew,
    TreeLightweight,
    TreeLightweightNew,
    TreeBackup,
    EthWatcher,
    // Eth tx generator
    EthTxAggregator,
    // Manager for eth tx
    EthTxManager,
    // Data fetchers: list fetcher, volume fetcher, price fetcher.
    DataFetcher,
    // State keeper.
    StateKeeper,
    // Witness Generator. The first argument is a number of jobs to process. If None, runs indefinitely.
    // The second argument is the type of the witness-generation performed
    WitnessGenerator(Option<usize>, AggregationRound),
    // Component for housekeeping task such as cleaning blobs from GCS, reporting metrics etc.
    Housekeeper,
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
                Component::ExplorerApi,
            ])),
            "http_api" => Ok(Components(vec![Component::HttpApi])),
            "ws_api" => Ok(Components(vec![Component::WsApi])),
            "explorer_api" => Ok(Components(vec![Component::ExplorerApi])),
            "tree" => Ok(Components(vec![Component::Tree])),
            "tree_new" => Ok(Components(vec![Component::TreeNew])),
            "tree_lightweight" => Ok(Components(vec![Component::TreeLightweight])),
            "tree_lightweight_new" => Ok(Components(vec![Component::TreeLightweightNew])),
            "tree_backup" => Ok(Components(vec![Component::TreeBackup])),
            "data_fetcher" => Ok(Components(vec![Component::DataFetcher])),
            "state_keeper" => Ok(Components(vec![Component::StateKeeper])),
            "housekeeper" => Ok(Components(vec![Component::Housekeeper])),
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
            other => Err(format!("{} is not a valid component name", other)),
        }
    }
}

pub async fn initialize_components(
    config: &ZkSyncConfig,
    components: Vec<Component>,
    use_prometheus_pushgateway: bool,
) -> anyhow::Result<(
    Vec<JoinHandle<()>>,
    watch::Sender<bool>,
    oneshot::Receiver<CircuitBreakerError>,
)> {
    vlog::info!("Starting the components: {:?}", components);
    let connection_pool = ConnectionPool::new(None, true);
    let replica_connection_pool = ConnectionPool::new(None, false);
    let mut healthchecks: Vec<Box<dyn CheckHealth>> = Vec::new();
    let circuit_breaker_checker = CircuitBreakerChecker::new(
        circuit_breakers_for_components(&components, config),
        &config.chain.circuit_breaker,
    );
    circuit_breaker_checker.check().await.unwrap_or_else(|err| {
        panic!("Circuit breaker triggered: {}", err);
    });

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (cb_sender, cb_receiver) = oneshot::channel();
    // Prometheus exporter and circuit breaker checker should run for every component configuration.
    let mut task_futures: Vec<JoinHandle<()>> = vec![
        run_prometheus_exporter(config.api.prometheus.clone(), use_prometheus_pushgateway),
        tokio::spawn(circuit_breaker_checker.run(cb_sender, stop_receiver.clone())),
    ];

    if components.contains(&Component::HttpApi) {
        let started_at = Instant::now();
        vlog::info!("initializing HTTP API");
        task_futures.extend(
            run_http_api(
                config,
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
            )
            .await,
        );
        vlog::info!("initialized HTTP API in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "http_api");
    }

    if components.contains(&Component::WsApi) {
        let started_at = Instant::now();
        vlog::info!("initializing WS API");
        task_futures.extend(
            run_ws_api(
                config,
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
            )
            .await,
        );
        vlog::info!("initialized WS API in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "ws_api");
    }

    if components.contains(&Component::ExplorerApi) {
        let started_at = Instant::now();
        vlog::info!("initializing explorer REST API");
        task_futures.push(explorer::start_server_thread_detached(
            config.api.explorer.clone(),
            config.contracts.l2_erc20_bridge_addr,
            config.chain.state_keeper.fee_account_addr,
            connection_pool.clone(),
            replica_connection_pool.clone(),
            stop_receiver.clone(),
        ));
        vlog::info!(
            "initialized explorer REST API in {:?}",
            started_at.elapsed()
        );
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "explorer_api");
    }

    if components.contains(&Component::StateKeeper) {
        let started_at = Instant::now();
        vlog::info!("initializing State Keeper");
        let state_keeper_pool = ConnectionPool::new(Some(1), true);
        let next_priority_id = state_keeper_pool
            .access_storage_blocking()
            .transactions_dal()
            .next_priority_id();
        let mempool = MempoolGuard(Arc::new(Mutex::new(MempoolStore::new(
            next_priority_id,
            config.chain.mempool.capacity,
        ))));
        let eth_gateway = PKSigningClient::from_config(config);
        let gas_adjuster = Arc::new(
            GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
                .await
                .unwrap(),
        );

        let bounded_gas_adjuster = Arc::new(BoundedGasAdjuster::new(
            config.chain.state_keeper.max_l1_gas_price(),
            gas_adjuster.clone(),
        ));
        task_futures.push(tokio::task::spawn(gas_adjuster.run(stop_receiver.clone())));

        let state_keeper_actor = state_keeper::start_state_keeper(
            config,
            &state_keeper_pool,
            mempool.clone(),
            bounded_gas_adjuster.clone(),
            stop_receiver.clone(),
        );

        task_futures.push(tokio::task::spawn_blocking(move || {
            state_keeper_actor.run()
        }));

        let mempool_fetcher_pool = ConnectionPool::new(Some(1), true);
        let mempool_fetcher_actor = MempoolFetcher::new(mempool, bounded_gas_adjuster, config);
        task_futures.push(tokio::spawn(mempool_fetcher_actor.run(
            mempool_fetcher_pool,
            config.chain.mempool.remove_stuck_txs,
            config.chain.mempool.stuck_tx_timeout(),
            config.chain.state_keeper.fair_l2_gas_price,
            stop_receiver.clone(),
        )));

        // Fee monitor is normally tied to a single instance of server, and it makes most sense to keep it together
        // with state keeper (since without state keeper running there should be no balance changes).
        let fee_monitor_eth_gateway = PKSigningClient::from_config(config);
        let fee_monitor_pool = ConnectionPool::new(Some(1), true);
        let fee_monitor_actor = FeeMonitor::new(config, fee_monitor_pool, fee_monitor_eth_gateway);
        task_futures.push(tokio::spawn(fee_monitor_actor.run()));

        vlog::info!("initialized State Keeper in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "state_keeper");
    }

    if components.contains(&Component::EthWatcher) {
        let started_at = Instant::now();
        vlog::info!("initializing ETH-Watcher");
        let eth_gateway = PKSigningClient::from_config(config);
        let eth_watch_pool = ConnectionPool::new(Some(1), true);
        task_futures.push(
            start_eth_watch(
                eth_watch_pool,
                eth_gateway.clone(),
                config,
                stop_receiver.clone(),
            )
            .await,
        );
        vlog::info!("initialized ETH-Watcher in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "eth_watcher");
    }

    if components.contains(&Component::EthTxAggregator) {
        let started_at = Instant::now();
        vlog::info!("initializing ETH-TxAggregator");
        let eth_sender_storage = ConnectionPool::new(Some(1), true);
        let eth_gateway = PKSigningClient::from_config(config);
        let nonce = eth_gateway.pending_nonce("eth_sender").await.unwrap();
        let eth_tx_aggregator_actor = EthTxAggregator::new(
            config.eth_sender.sender.clone(),
            Aggregator::new(config.eth_sender.sender.clone()),
            config.contracts.validator_timelock_addr,
            nonce.as_u64(),
        );
        task_futures.push(tokio::spawn(eth_tx_aggregator_actor.run(
            eth_sender_storage.clone(),
            eth_gateway.clone(),
            stop_receiver.clone(),
        )));
        vlog::info!("initialized ETH-TxAggregator in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "eth_tx_aggregator");
    }

    if components.contains(&Component::EthTxManager) {
        let started_at = Instant::now();
        vlog::info!("initializing ETH-TxManager");
        let eth_sender_storage = ConnectionPool::new(Some(1), true);
        let eth_gateway = PKSigningClient::from_config(config);
        let gas_adjuster = Arc::new(
            GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
                .await
                .unwrap(),
        );
        let eth_tx_manager_actor = EthTxManager::new(
            config.eth_sender.sender.clone(),
            gas_adjuster.clone(),
            eth_gateway.clone(),
        );
        task_futures.extend([
            tokio::spawn(
                eth_tx_manager_actor.run(eth_sender_storage.clone(), stop_receiver.clone()),
            ),
            tokio::spawn(gas_adjuster.run(stop_receiver.clone())),
        ]);
        vlog::info!("initialized ETH-TxManager in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "eth_tx_aggregator");
    }

    if components.contains(&Component::DataFetcher) {
        let started_at = Instant::now();
        vlog::info!("initializing data fetchers");
        task_futures.extend(run_data_fetchers(
            config,
            connection_pool.clone(),
            stop_receiver.clone(),
        ));
        vlog::info!("initialized data fetchers in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed(), "stage" => "data_fetchers");
    }

    let store_factory = ObjectStoreFactory::from_env();
    add_trees_to_task_futures(
        &components,
        config,
        &store_factory,
        &stop_receiver,
        &mut task_futures,
        &mut healthchecks,
    );
    add_witness_generator_to_task_futures(
        &components,
        &connection_pool,
        &store_factory,
        &stop_receiver,
        &mut task_futures,
    );

    if components.contains(&Component::Housekeeper) {
        let house_keeper_config = HouseKeeperConfig::from_env();
        let l1_batch_metrics_reporter =
            L1BatchMetricsReporter::new(house_keeper_config.l1_batch_metrics_reporting_interval_ms);
        let gcs_blob_cleaner = GcsBlobCleaner::new(
            &store_factory,
            house_keeper_config.blob_cleaning_interval_ms,
        );
        let gpu_prover_queue = GpuProverQueueMonitor::new(
            ProverGroupConfig::from_env().synthesizer_per_gpu,
            house_keeper_config.gpu_prover_queue_reporting_interval_ms,
        );
        let config = ProverConfigs::from_env().non_gpu;
        let prover_job_retry_manager = ProverJobRetryManager::new(
            config.max_attempts,
            config.proof_generation_timeout(),
            house_keeper_config.prover_job_retrying_interval_ms,
        );
        let prover_stats_reporter =
            ProverStatsReporter::new(house_keeper_config.prover_stats_reporting_interval_ms);
        let waiting_to_queued_witness_job_mover =
            WaitingToQueuedWitnessJobMover::new(house_keeper_config.witness_job_moving_interval_ms);
        let witness_generator_stats_reporter = WitnessGeneratorStatsReporter::new(
            house_keeper_config.witness_generator_stats_reporting_interval_ms,
        );

        let witness_generator_metrics = [
            tokio::spawn(witness_generator_stats_reporter.run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(gpu_prover_queue.run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(gcs_blob_cleaner.run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(l1_batch_metrics_reporter.run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(prover_stats_reporter.run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(
                waiting_to_queued_witness_job_mover.run(ConnectionPool::new(Some(1), true)),
            ),
            tokio::spawn(prover_job_retry_manager.run(ConnectionPool::new(Some(1), true))),
        ];

        task_futures.extend(witness_generator_metrics);
    }

    // Run healthcheck server for all components.
    healthchecks.push(Box::new(ConnectionPoolHealthCheck::new(
        replica_connection_pool,
    )));
    task_futures.push(healthcheck::start_server_thread_detached(
        config.api.healthcheck.bind_addr(),
        healthchecks,
        stop_receiver,
    ));

    Ok((task_futures, stop_sender, cb_receiver))
}

fn add_trees_to_task_futures(
    components: &[Component],
    config: &ZkSyncConfig,
    store_factory: &ObjectStoreFactory,
    stop_receiver: &watch::Receiver<bool>,
    task_futures: &mut Vec<JoinHandle<()>>,
    healthchecks: &mut Vec<Box<dyn CheckHealth>>,
) {
    const COMPONENTS_TO_MODES: &[(Component, bool, TreeImplementation)] = &[
        (Component::Tree, true, TreeImplementation::Old),
        (Component::TreeNew, true, TreeImplementation::New),
        (Component::TreeLightweight, false, TreeImplementation::Old),
        (
            Component::TreeLightweightNew,
            false,
            TreeImplementation::New,
        ),
    ];

    if components.contains(&Component::TreeBackup) {
        panic!("Tree backup mode is disabled");
    }
    if components.contains(&Component::TreeNew)
        && components.contains(&Component::TreeLightweightNew)
    {
        panic!(
            "Cannot start a node with a new tree in both full and lightweight modes. \
             Since the storage layout is mode-independent, choose either of modes and run \
             the node with it."
        );
    }

    for &(component, is_full, implementation) in COMPONENTS_TO_MODES {
        if components.contains(&component) {
            let store_factory = is_full.then_some(store_factory);
            let (future, tree_health_check) =
                run_tree(config, store_factory, stop_receiver.clone(), implementation);
            task_futures.push(future);
            healthchecks.push(Box::new(tree_health_check));
        }
    }
}

fn run_tree(
    config: &ZkSyncConfig,
    store_factory: Option<&ObjectStoreFactory>,
    stop_receiver: watch::Receiver<bool>,
    implementation: TreeImplementation,
) -> (JoinHandle<()>, TreeHealthCheck) {
    let started_at = Instant::now();
    vlog::info!(
        "initializing Merkle tree with {:?} implementation in {} mode",
        implementation,
        if store_factory.is_some() {
            "full"
        } else {
            "lightweight"
        }
    );

    let metadata_calculator = if let Some(factory) = store_factory {
        MetadataCalculator::full(config, factory, implementation)
    } else {
        MetadataCalculator::lightweight(config, implementation)
    };
    let tree_health_check = metadata_calculator.tree_health_check();
    let tree_tag = metadata_calculator.tree_tag();
    let future = tokio::task::spawn_blocking(|| {
        let pool = ConnectionPool::new(Some(1), true);
        metadata_calculator.run(&pool, stop_receiver);
    });

    vlog::info!(
        "initialized `{}` tree in {:?}",
        tree_tag,
        started_at.elapsed()
    );
    metrics::gauge!(
        "server.init.latency",
        started_at.elapsed(),
        "stage" => "tree",
        "tree" => tree_tag
    );
    (future, tree_health_check)
}

fn add_witness_generator_to_task_futures(
    components: &[Component],
    connection_pool: &ConnectionPool,
    store_factory: &ObjectStoreFactory,
    stop_receiver: &watch::Receiver<bool>,
    task_futures: &mut Vec<JoinHandle<()>>,
) {
    // We don't want witness generator to run on local nodes, as it's CPU heavy.
    if std::env::var("ZKSYNC_LOCAL_SETUP") == Ok("true".to_owned()) {
        return;
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
        vlog::info!(
            "initializing the {:?} witness generator, batch size: {:?}",
            component_type,
            batch_size
        );

        let config = WitnessGeneratorConfig::from_env();
        let task = match component_type {
            AggregationRound::BasicCircuits => {
                let witness_generator = BasicWitnessGenerator::new(config, store_factory);
                tokio::spawn(witness_generator.run(
                    connection_pool.clone(),
                    stop_receiver.clone(),
                    batch_size,
                ))
            }
            AggregationRound::LeafAggregation => {
                let witness_generator = LeafAggregationWitnessGenerator::new(config, store_factory);
                tokio::spawn(witness_generator.run(
                    connection_pool.clone(),
                    stop_receiver.clone(),
                    batch_size,
                ))
            }
            AggregationRound::NodeAggregation => {
                let witness_generator = NodeAggregationWitnessGenerator::new(config, store_factory);
                tokio::spawn(witness_generator.run(
                    connection_pool.clone(),
                    stop_receiver.clone(),
                    batch_size,
                ))
            }
            AggregationRound::Scheduler => {
                let witness_generator = SchedulerWitnessGenerator::new(config, store_factory);
                tokio::spawn(witness_generator.run(
                    connection_pool.clone(),
                    stop_receiver.clone(),
                    batch_size,
                ))
            }
        };
        task_futures.push(task);

        vlog::info!(
            "initialized {:?} witness generator in {:?}",
            component_type,
            started_at.elapsed()
        );
        metrics::gauge!(
            "server.init.latency",
            started_at.elapsed(),
            "stage" => format!("witness_generator_{:?}", component_type)
        );
    }
}

fn build_tx_sender<G: L1GasPriceProvider>(
    config: &ZkSyncConfig,
    replica_pool: ConnectionPool,
    master_pool: ConnectionPool,
    l1_gas_price_provider: Arc<G>,
) -> TxSender<G> {
    let mut tx_sender_builder = TxSenderBuilder::new(config.clone().into(), replica_pool)
        .with_main_connection_pool(master_pool)
        .with_state_keeper_config(config.chain.state_keeper.clone());

    // Add rate limiter if enabled.
    if let Some(transactions_per_sec_limit) = config.api.web3_json_rpc.transactions_per_sec_limit {
        tx_sender_builder = tx_sender_builder.with_rate_limiter(transactions_per_sec_limit);
    };

    tx_sender_builder.build(
        l1_gas_price_provider,
        config.chain.state_keeper.default_aa_hash,
    )
}

async fn run_http_api(
    config: &ZkSyncConfig,
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> Vec<JoinHandle<()>> {
    let eth_gateway = PKSigningClient::from_config(config);
    let gas_adjuster = Arc::new(
        GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
            .await
            .unwrap(),
    );
    let bounded_gas_adjuster = Arc::new(BoundedGasAdjuster::new(
        config.chain.state_keeper.max_l1_gas_price(),
        gas_adjuster.clone(),
    ));

    let tx_sender = build_tx_sender(
        config,
        replica_connection_pool.clone(),
        master_connection_pool.clone(),
        bounded_gas_adjuster,
    );

    let mut handles = {
        let mut builder =
            web3::ApiBuilder::jsonrpc_backend(config.clone().into(), replica_connection_pool)
                .http(config.api.web3_json_rpc.http_port)
                .with_filter_limit(config.api.web3_json_rpc.filters_limit())
                .with_threads(config.api.web3_json_rpc.threads_per_server as usize)
                .with_tx_sender(tx_sender);

        if config.chain.state_keeper.save_call_traces {
            builder = builder.enable_debug_namespace(
                config.chain.state_keeper.base_system_contracts_hashes(),
                config.chain.state_keeper.fair_l2_gas_price,
                config.api.web3_json_rpc.vm_execution_cache_misses_limit,
            )
        }

        builder.build(stop_receiver.clone())
    };

    handles.push(tokio::spawn(gas_adjuster.run(stop_receiver)));
    handles
}

async fn run_ws_api(
    config: &ZkSyncConfig,
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> Vec<JoinHandle<()>> {
    let eth_gateway = PKSigningClient::from_config(config);
    let gas_adjuster = Arc::new(
        GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
            .await
            .unwrap(),
    );

    let bounded_gas_adjuster = Arc::new(BoundedGasAdjuster::new(
        config.chain.state_keeper.max_l1_gas_price(),
        gas_adjuster.clone(),
    ));

    let tx_sender = build_tx_sender(
        config,
        replica_connection_pool.clone(),
        master_connection_pool.clone(),
        bounded_gas_adjuster,
    );

    let mut tasks =
        web3::ApiBuilder::jsonrpc_backend(config.clone().into(), replica_connection_pool)
            .ws(config.api.web3_json_rpc.ws_port)
            .with_filter_limit(config.api.web3_json_rpc.filters_limit())
            .with_subscriptions_limit(config.api.web3_json_rpc.subscriptions_limit())
            .with_polling_interval(config.api.web3_json_rpc.pubsub_interval())
            .with_tx_sender(tx_sender)
            .build(stop_receiver.clone());

    tasks.push(tokio::spawn(gas_adjuster.run(stop_receiver)));
    tasks
}

fn circuit_breakers_for_components(
    components: &[Component],
    config: &ZkSyncConfig,
) -> Vec<Box<dyn CircuitBreaker>> {
    let mut circuit_breakers: Vec<Box<dyn CircuitBreaker>> = Vec::new();

    if components.iter().any(|c| {
        matches!(
            c,
            Component::EthTxAggregator | Component::EthTxManager | Component::StateKeeper
        )
    }) {
        circuit_breakers.push(Box::new(FailedL1TransactionChecker {
            pool: ConnectionPool::new(Some(1), false),
        }));
    }

    if components.iter().any(|c| {
        matches!(
            c,
            Component::EthTxAggregator
                | Component::EthTxManager
                | Component::Tree
                | Component::TreeBackup
        )
    }) {
        let eth_client = PKSigningClient::from_config(config);
        circuit_breakers.push(Box::new(VksChecker::new(
            &config.chain.circuit_breaker,
            eth_client,
        )));
    }

    if components
        .iter()
        .any(|c| matches!(c, Component::EthTxAggregator | Component::EthTxManager))
    {
        let eth_client = PKSigningClient::from_config(config);
        circuit_breakers.push(Box::new(FacetSelectorsChecker::new(
            &config.chain.circuit_breaker,
            eth_client,
        )));
    }

    circuit_breakers
}

pub fn block_on<F: Future + Send + 'static>(future: F) -> F::Output
where
    F::Output: Send,
{
    std::thread::spawn(move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime creation failed");
        runtime.block_on(future)
    })
    .join()
    .unwrap()
}

#[tokio::test]
async fn test_house_keeper_components_get_added() {
    let config = ZkSyncConfig::from_env();
    let (core_task_handles, _, _) =
        initialize_components(&config, vec![Component::Housekeeper], false)
            .await
            .unwrap();
    // circuit-breaker, prometheus-exporter, healthcheck components are run, irrespective of other components.
    let always_running_component_count = 3;
    assert_eq!(7, core_task_handles.len() - always_running_component_count);
}
