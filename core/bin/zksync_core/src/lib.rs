#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use futures::channel::oneshot;
use futures::future;
use std::time::Instant;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use zksync_config::configs::WitnessGeneratorConfig;

use house_keeper::periodic_job::PeriodicJob;
use prometheus_exporter::run_prometheus_exporter;
use zksync_circuit_breaker::{
    code_hashes::CodeHashesChecker, facet_selectors::FacetSelectorsChecker,
    l1_txs::FailedL1TransactionChecker, vks::VksChecker, CircuitBreaker, CircuitBreakerChecker,
    CircuitBreakerError,
};
use zksync_config::ZkSyncConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_eth_client::EthInterface;
use zksync_mempool::MempoolStore;
use zksync_object_store::object_store::create_object_store_from_env;
use zksync_queued_job_processor::JobProcessor;

use crate::eth_sender::{Aggregator, EthTxManager};
use crate::fee_monitor::FeeMonitor;
use crate::house_keeper::gcs_blob_cleaner::GcsBlobCleaner;
use crate::house_keeper::gpu_prover_queue_monitor::GpuProverQueueMonitor;
use crate::house_keeper::{
    witness_generator_misc_reporter::WitnessGeneratorMetricsReporter,
    witness_generator_queue_monitor::WitnessGeneratorStatsReporter,
};
use crate::metadata_calculator::{MetadataCalculator, MetadataCalculatorMode};
use crate::state_keeper::{MempoolFetcher, MempoolGuard};
use crate::witness_generator::WitnessGenerator;
use crate::{
    api_server::{explorer, web3},
    data_fetchers::run_data_fetchers,
    eth_sender::EthTxAggregator,
    eth_watch::start_eth_watch,
    gas_adjuster::GasAdjuster,
};

pub mod api_server;
pub mod data_fetchers;
pub mod db_storage_provider;
pub mod eth_sender;
pub mod eth_watch;
pub mod fee_monitor;
pub mod fee_ticker;
pub mod gas_adjuster;
pub mod gas_tracker;
pub mod genesis;
pub mod house_keeper;
pub mod metadata_calculator;
pub mod state_keeper;
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
                vlog::info!(
                    "One of the actors finished its run, while it wasn't expected to do it"
                );
            }
        }
        Err(error) => {
            vlog::info!(
                "One of the tokio actors unexpectedly finished, shutting down: {:?}",
                error
            );
        }
    }
}

/// Inserts the initial information about zkSync tokens into the database.
pub async fn genesis_init(config: ZkSyncConfig) {
    let mut storage = StorageProcessor::establish_connection(true).await;
    genesis::ensure_genesis_state(&mut storage, config).await;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Component {
    // Public Web3 API running on HTTP server.
    HttpApi,
    // Public Web3 API (including PubSub) running on WebSocket server.
    WsApi,
    // REST API for explorer.
    ExplorerApi,
    // Metadata Calculator.
    Tree,
    TreeLightweight,
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
    // Witness Generator. The argument is a number of jobs to process. If None, runs indefinitely.
    WitnessGenerator(Option<usize>),
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
            "tree_lightweight" => Ok(Components(vec![Component::TreeLightweight])),
            "tree_backup" => Ok(Components(vec![Component::TreeBackup])),
            "data_fetcher" => Ok(Components(vec![Component::DataFetcher])),
            "state_keeper" => Ok(Components(vec![Component::StateKeeper])),
            "housekeeper" => Ok(Components(vec![Component::Housekeeper])),
            "witness_generator" => Ok(Components(vec![Component::WitnessGenerator(None)])),
            "one_shot_witness_generator" => {
                Ok(Components(vec![Component::WitnessGenerator(Some(1))]))
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
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "http_api");
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
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "ws_api");
    }

    if components.contains(&Component::ExplorerApi) {
        let started_at = Instant::now();
        vlog::info!("initializing explorer REST API");
        task_futures.push(explorer::start_server_thread_detached(
            config,
            connection_pool.clone(),
            replica_connection_pool,
            stop_receiver.clone(),
        ));
        vlog::info!(
            "initialized explorer REST API in {:?}",
            started_at.elapsed()
        );
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "explorer_api");
    }

    if components.contains(&Component::StateKeeper) {
        let started_at = Instant::now();
        vlog::info!("initializing State Keeper");
        let state_keeper_pool = ConnectionPool::new(Some(1), true);
        let next_priority_id = state_keeper_pool
            .access_storage()
            .await
            .transactions_dal()
            .next_priority_id();
        let mempool = MempoolGuard(Arc::new(Mutex::new(MempoolStore::new(
            next_priority_id,
            config.chain.mempool.capacity,
        ))));
        let eth_gateway = EthereumClient::from_config(config);
        let gas_adjuster = Arc::new(
            GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
                .await
                .unwrap(),
        );
        task_futures.push(tokio::task::spawn(
            gas_adjuster.clone().run(stop_receiver.clone()),
        ));

        let state_keeper_actor = crate::state_keeper::start_state_keeper(
            config,
            &state_keeper_pool,
            mempool.clone(),
            gas_adjuster.clone(),
            stop_receiver.clone(),
        );

        task_futures.push(tokio::task::spawn_blocking(move || {
            state_keeper_actor.run()
        }));

        let mempool_fetcher_pool = ConnectionPool::new(Some(1), true);
        let mempool_fetcher_actor = MempoolFetcher::new(mempool, gas_adjuster, config);
        task_futures.push(tokio::spawn(mempool_fetcher_actor.run(
            mempool_fetcher_pool,
            config.chain.mempool.remove_stuck_txs,
            config.chain.mempool.stuck_tx_timeout(),
            stop_receiver.clone(),
        )));

        // Fee monitor is normally tied to a single instance of server, and it makes most sense to keep it together
        // with state keeper (since without state keeper running there should be no balance changes).
        let fee_monitor_eth_gateway = EthereumClient::from_config(config);
        let fee_monitor_pool = ConnectionPool::new(Some(1), true);
        let fee_monitor_actor =
            FeeMonitor::new(config, fee_monitor_pool, fee_monitor_eth_gateway).await;
        task_futures.push(tokio::spawn(fee_monitor_actor.run()));

        vlog::info!("initialized State Keeper in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "state_keeper");
    }

    if components.contains(&Component::EthWatcher) {
        let started_at = Instant::now();
        vlog::info!("initializing ETH-Watcher");
        let eth_gateway = EthereumClient::from_config(config);
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
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "eth_watcher");
    }

    if components.contains(&Component::EthTxAggregator) {
        let started_at = Instant::now();
        vlog::info!("initializing ETH-TxAggregator");
        let eth_sender_storage = ConnectionPool::new(Some(1), true);
        let eth_gateway = EthereumClient::from_config(config);
        let nonce = eth_gateway.pending_nonce("eth_sender").await.unwrap();
        let eth_tx_aggregator_actor = EthTxAggregator::new(
            config.eth_sender.sender.clone(),
            Aggregator::new(config.eth_sender.sender.clone()),
            config.contracts.diamond_proxy_addr,
            nonce.as_u64(),
        );
        task_futures.push(tokio::spawn(
            eth_tx_aggregator_actor.run(eth_sender_storage.clone(), stop_receiver.clone()),
        ));
        vlog::info!("initialized ETH-TxAggregator in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "eth_tx_aggregator");
    }

    if components.contains(&Component::EthTxManager) {
        let started_at = Instant::now();
        vlog::info!("initializing ETH-TxManager");
        let eth_sender_storage = ConnectionPool::new(Some(1), true);
        let eth_gateway = EthereumClient::from_config(config);
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
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "eth_tx_aggregator");
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
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "data_fetchers");
    }

    if components.contains(&Component::Tree) {
        let started_at = Instant::now();
        vlog::info!("initializing the tree");
        task_futures.extend(run_tree(
            config,
            stop_receiver.clone(),
            MetadataCalculatorMode::Full,
        ));
        vlog::info!("initialized tree in {:?}", started_at.elapsed());
        metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "tree");
    }

    if components.contains(&Component::TreeLightweight) {
        task_futures.extend(run_tree(
            config,
            stop_receiver.clone(),
            MetadataCalculatorMode::Lightweight,
        ));
    }

    if components.contains(&Component::TreeBackup) {
        task_futures.extend(run_tree(
            config,
            stop_receiver.clone(),
            MetadataCalculatorMode::Backup,
        ));
    }

    // We don't want witness generator to run on local nodes, as it's CPU heavy and is not stable yet
    let is_local_setup = std::env::var("ZKSYNC_LOCAL_SETUP") == Ok("true".to_owned());
    if let Some(Component::WitnessGenerator(batch_size)) = components
        .iter()
        .find(|c| matches!(c, Component::WitnessGenerator(_)))
    {
        if !is_local_setup {
            let started_at = Instant::now();
            vlog::info!(
                "initializing the witness generator, batch size: {:?}",
                batch_size
            );
            let config = WitnessGeneratorConfig::from_env();
            let witness_generator = WitnessGenerator::new(config);
            task_futures.push(tokio::spawn(witness_generator.run(
                connection_pool.clone(),
                stop_receiver.clone(),
                *batch_size,
            )));
            vlog::info!(
                "initialized witness generator in {:?}",
                started_at.elapsed()
            );
            metrics::gauge!("server.init.latency", started_at.elapsed().as_secs() as f64, "stage" => "witness_generator");
        }
    }

    if components.contains(&Component::Housekeeper) {
        let witness_generator_misc_reporter = WitnessGeneratorMetricsReporter {
            witness_generator_config: WitnessGeneratorConfig::from_env(),
            prover_config: config.prover.non_gpu.clone(),
        };
        let gcs_blob_cleaner = GcsBlobCleaner {
            object_store: create_object_store_from_env(),
        };
        let witness_generator_metrics = vec![
            tokio::spawn(
                WitnessGeneratorStatsReporter::default().run(ConnectionPool::new(Some(1), true)),
            ),
            tokio::spawn(witness_generator_misc_reporter.run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(GpuProverQueueMonitor::default().run(ConnectionPool::new(Some(1), true))),
            tokio::spawn(gcs_blob_cleaner.run(ConnectionPool::new(Some(1), true))),
        ];

        task_futures.extend(witness_generator_metrics);
    }

    Ok((task_futures, stop_sender, cb_receiver))
}

fn run_tree(
    config: &ZkSyncConfig,
    stop_receiver: watch::Receiver<bool>,
    mode: MetadataCalculatorMode,
) -> Vec<JoinHandle<()>> {
    let metadata_calculator = MetadataCalculator::new(config, mode);
    let pool = ConnectionPool::new(Some(1), true);
    vec![tokio::spawn(metadata_calculator.run(pool, stop_receiver))]
}

async fn run_http_api(
    config: &ZkSyncConfig,
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> Vec<JoinHandle<()>> {
    let eth_gateway = EthereumClient::from_config(config);
    let gas_adjuster = Arc::new(
        GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
            .await
            .unwrap(),
    );
    vec![
        web3::start_http_rpc_server_old(
            master_connection_pool,
            replica_connection_pool,
            config,
            stop_receiver.clone(),
            gas_adjuster.clone(),
        ),
        tokio::spawn(gas_adjuster.run(stop_receiver)),
    ]
}

async fn run_ws_api(
    config: &ZkSyncConfig,
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> Vec<JoinHandle<()>> {
    let eth_gateway = EthereumClient::from_config(config);
    let gas_adjuster = Arc::new(
        GasAdjuster::new(eth_gateway.clone(), config.eth_sender.gas_adjuster)
            .await
            .unwrap(),
    );
    web3::start_ws_rpc_server_old(
        master_connection_pool,
        replica_connection_pool,
        config,
        stop_receiver,
        gas_adjuster,
    )
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
                | Component::StateKeeper
                | Component::Tree
                | Component::TreeBackup
        )
    }) {
        circuit_breakers.push(Box::new(CodeHashesChecker::new(config)));
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
        circuit_breakers.push(Box::new(VksChecker::new(config)));
    }

    if components
        .iter()
        .any(|c| matches!(c, Component::EthTxAggregator | Component::EthTxManager))
    {
        circuit_breakers.push(Box::new(FacetSelectorsChecker::new(config)));
    }

    circuit_breakers
}
