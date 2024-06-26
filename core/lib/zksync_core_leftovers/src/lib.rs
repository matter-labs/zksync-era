#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::{
    net::Ipv4Addr,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::Prover;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use zksync_circuit_breaker::{
    l1_txs::FailedL1TransactionChecker, replication_lag::ReplicationLagChecker,
    CircuitBreakerChecker, CircuitBreakers,
};
use zksync_commitment_generator::{
    validation_task::L1BatchCommitmentModeValidationTask, CommitmentGenerator,
};
use zksync_concurrency::{ctx, scope};
use zksync_config::{
    configs::{
        api::{MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{CircuitBreakerConfig, MempoolConfig, OperationsManagerConfig, StateKeeperConfig},
        consensus::ConsensusConfig,
        database::{MerkleTreeConfig, MerkleTreeMode},
        wallets,
        wallets::Wallets,
        ContractsConfig, DatabaseSecrets, GeneralConfig, Secrets,
    },
    ApiConfig, DBConfig, EthWatchConfig, GenesisConfig,
};
use zksync_contracts::governance_contract;
use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core, CoreDal};
use zksync_db_connection::healthcheck::ConnectionPoolHealthCheck;
use zksync_eth_client::{clients::PKSigningClient, BoundEthInterface};
use zksync_eth_sender::{Aggregator, EthTxAggregator, EthTxManager};
use zksync_eth_watch::{EthHttpQueryClient, EthWatch};
use zksync_health_check::{AppHealthCheck, HealthStatus, ReactiveHealthCheck};
use zksync_house_keeper::{
    blocks_state_reporter::L1BatchMetricsReporter,
    periodic_job::PeriodicJob,
    prover::{
        FriGpuProverArchiver, FriProofCompressorJobRetryManager, FriProofCompressorQueueReporter,
        FriProverJobRetryManager, FriProverJobsArchiver, FriProverQueueReporter,
        FriWitnessGeneratorJobRetryManager, FriWitnessGeneratorQueueReporter,
        WaitingToQueuedFriWitnessJobMover,
    },
};
use zksync_metadata_calculator::{
    api_server::TreeApiHttpClient, MetadataCalculator, MetadataCalculatorConfig,
};
use zksync_node_api_server::{
    healthcheck::HealthCheckHandle,
    tx_sender::{build_tx_sender, TxSenderConfig},
    web3::{self, mempool_cache::MempoolCache, state::InternalApiConfig, Namespace},
};
use zksync_node_fee_model::{
    l1_gas_price::GasAdjusterSingleton, BatchFeeModelInputProvider, MainNodeFeeInputProvider,
};
use zksync_node_genesis::{ensure_genesis_state, GenesisParams};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_shared_metrics::{InitStage, APP_METRICS};
use zksync_state::{PostgresStorageCaches, RocksdbStorageOptions};
use zksync_state_keeper::{
    create_state_keeper, io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess,
    AsyncRocksdbCache, MempoolFetcher, MempoolGuard, OutputHandler, StateKeeperPersistence,
    TreeWritesPersistence,
};
use zksync_tee_verifier_input_producer::TeeVerifierInputProducer;
use zksync_types::{ethabi::Contract, fee_model::FeeModelConfig, Address, L2ChainId};
use zksync_web3_decl::client::{Client, DynClient, L1};

pub mod temp_config_store;

/// Inserts the initial information about ZKsync tokens into the database.
pub async fn genesis_init(
    genesis_config: GenesisConfig,
    database_secrets: &DatabaseSecrets,
) -> anyhow::Result<()> {
    let db_url = database_secrets.master_url()?;
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .context("failed to build connection_pool")?;
    let mut storage = pool.connection().await.context("connection()")?;

    let params = GenesisParams::load_genesis_params(genesis_config)?;
    ensure_genesis_state(&mut storage, &params).await?;

    Ok(())
}

pub async fn is_genesis_needed(database_secrets: &DatabaseSecrets) -> bool {
    let db_url = database_secrets.master_url().unwrap();
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .expect("failed to build connection_pool");
    let mut storage = pool.connection().await.expect("connection()");
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
    /// Public Web3 API (including PubSub) running on WebSocket server.
    WsApi,
    /// REST API for contract verification.
    ContractVerificationApi,
    /// Metadata calculator.
    Tree,
    /// Merkle tree API.
    TreeApi,
    EthWatcher,
    /// Eth tx generator.
    EthTxAggregator,
    /// Manager for eth tx.
    EthTxManager,
    /// State keeper.
    StateKeeper,
    /// Produces input for the TEE verifier.
    /// The blob is later used as input for TEE verifier.
    TeeVerifierInputProducer,
    /// Component for housekeeping task such as cleaning blobs from GCS, reporting metrics etc.
    Housekeeper,
    /// Component for exposing APIs to prover for providing proof generation data and accepting proofs.
    ProofDataHandler,
    /// Component generating BFT consensus certificates for L2 blocks.
    Consensus,
    /// Component generating commitment for L1 batches.
    CommitmentGenerator,
    /// VM runner-based component that saves protective reads to Postgres.
    VmRunnerProtectiveReads,
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
            "ws_api" => Ok(Components(vec![Component::WsApi])),
            "contract_verification_api" => Ok(Components(vec![Component::ContractVerificationApi])),
            "tree" => Ok(Components(vec![Component::Tree])),
            "tree_api" => Ok(Components(vec![Component::TreeApi])),
            "state_keeper" => Ok(Components(vec![Component::StateKeeper])),
            "housekeeper" => Ok(Components(vec![Component::Housekeeper])),
            "tee_verifier_input_producer" => {
                Ok(Components(vec![Component::TeeVerifierInputProducer]))
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
            "consensus" => Ok(Components(vec![Component::Consensus])),
            "commitment_generator" => Ok(Components(vec![Component::CommitmentGenerator])),
            "vm_runner_protective_reads" => {
                Ok(Components(vec![Component::VmRunnerProtectiveReads]))
            }
            other => Err(format!("{} is not a valid component name", other)),
        }
    }
}

pub async fn initialize_components(
    configs: &GeneralConfig,
    wallets: &Wallets,
    genesis_config: &GenesisConfig,
    contracts_config: &ContractsConfig,
    components: &[Component],
    secrets: &Secrets,
    consensus_config: Option<ConsensusConfig>,
) -> anyhow::Result<(
    Vec<JoinHandle<anyhow::Result<()>>>,
    watch::Sender<bool>,
    HealthCheckHandle,
)> {
    tracing::info!("Starting the components: {components:?}");
    let l2_chain_id = genesis_config.l2_chain_id;
    let db_config = configs.db_config.clone().context("db_config")?;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;
    let database_secrets = secrets.database.clone().context("database_secrets")?;

    if let Some(threshold) = postgres_config.slow_query_threshold() {
        ConnectionPool::<Core>::global_config().set_slow_query_threshold(threshold)?;
    }
    if let Some(threshold) = postgres_config.long_connection_threshold() {
        ConnectionPool::<Core>::global_config().set_long_connection_threshold(threshold)?;
    }

    let pool_size = postgres_config.max_connections()?;
    let pool_size_master = postgres_config
        .max_connections_master()
        .unwrap_or(pool_size);

    let connection_pool =
        ConnectionPool::<Core>::builder(database_secrets.master_url()?, pool_size_master)
            .build()
            .await
            .context("failed to build connection_pool")?;
    // We're most interested in setting acquire / statement timeouts for the API server, which puts the most load
    // on Postgres.
    let replica_connection_pool =
        ConnectionPool::<Core>::builder(database_secrets.replica_url()?, pool_size)
            .set_acquire_timeout(postgres_config.acquire_timeout())
            .set_statement_timeout(postgres_config.statement_timeout())
            .build()
            .await
            .context("failed to build replica_connection_pool")?;

    let health_check_config = configs
        .api_config
        .clone()
        .context("api_config")?
        .healthcheck;

    let app_health = Arc::new(AppHealthCheck::new(
        health_check_config.slow_time_limit(),
        health_check_config.hard_time_limit(),
    ));

    let eth = configs.eth.clone().context("eth")?;
    let l1_secrets = secrets.l1.clone().context("l1_secrets")?;
    let circuit_breaker_config = configs
        .circuit_breaker_config
        .clone()
        .context("circuit_breaker_config")?;

    let circuit_breaker_checker = CircuitBreakerChecker::new(
        Arc::new(
            circuit_breakers_for_components(components, &database_secrets, &circuit_breaker_config)
                .await
                .context("circuit_breakers_for_components")?,
        ),
        circuit_breaker_config.sync_interval(),
    );
    circuit_breaker_checker.check().await.unwrap_or_else(|err| {
        panic!("Circuit breaker triggered: {}", err);
    });

    let query_client = Client::http(l1_secrets.l1_rpc_url.clone())
        .context("Ethereum client")?
        .for_network(genesis_config.l1_chain_id.into())
        .build();
    let query_client = Box::new(query_client);
    let gas_adjuster_config = eth.gas_adjuster.context("gas_adjuster")?;
    let sender = eth.sender.as_ref().context("sender")?;

    let mut gas_adjuster = GasAdjusterSingleton::new(
        genesis_config.l1_chain_id,
        l1_secrets.l1_rpc_url.clone(),
        gas_adjuster_config,
        sender.pubdata_sending_mode,
        genesis_config.l1_batch_commit_data_generator_mode,
    );

    let (stop_sender, stop_receiver) = watch::channel(false);

    // Prometheus exporter and circuit breaker checker should run for every component configuration.
    let prom_config = configs
        .prometheus_config
        .clone()
        .context("prometheus_config")?;
    let prom_config = PrometheusExporterConfig::pull(prom_config.listener_port);

    let (prometheus_health_check, prometheus_health_updater) =
        ReactiveHealthCheck::new("prometheus_exporter");
    app_health.insert_component(prometheus_health_check)?;
    let prometheus_task = prom_config.run(stop_receiver.clone());
    let prometheus_task = tokio::spawn(async move {
        prometheus_health_updater.update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(prometheus_health_updater);
        res
    });

    let mut task_futures: Vec<JoinHandle<anyhow::Result<()>>> = vec![
        prometheus_task,
        tokio::spawn(circuit_breaker_checker.run(stop_receiver.clone())),
    ];

    if components.contains(&Component::WsApi)
        || components.contains(&Component::HttpApi)
        || components.contains(&Component::ContractVerificationApi)
    {
        let api_config = configs.api_config.clone().context("api_config")?;
        let state_keeper_config = configs
            .state_keeper_config
            .clone()
            .context("state_keeper_config")?;
        let tx_sender_config = TxSenderConfig::new(
            &state_keeper_config,
            &api_config.web3_json_rpc,
            wallets
                .state_keeper
                .clone()
                .context("Fee account")?
                .fee_account
                .address(),
            l2_chain_id,
        );
        let internal_api_config =
            InternalApiConfig::new(&api_config.web3_json_rpc, contracts_config, genesis_config);

        // Lazily initialize storage caches only when they are needed (e.g., skip their initialization
        // if we only run the explorer APIs). This is required because the cache update task will
        // terminate immediately if storage caches are dropped, which will lead to the (unexpected)
        // program termination.
        let mut storage_caches = None;

        let mempool_cache = MempoolCache::new(api_config.web3_json_rpc.mempool_cache_size());
        let mempool_cache_update_task = mempool_cache.update_task(
            connection_pool.clone(),
            api_config.web3_json_rpc.mempool_cache_update_interval(),
        );
        task_futures.push(tokio::spawn(
            mempool_cache_update_task.run(stop_receiver.clone()),
        ));

        if components.contains(&Component::HttpApi) {
            storage_caches = Some(
                build_storage_caches(
                    &api_config.web3_json_rpc,
                    &replica_connection_pool,
                    &mut task_futures,
                    stop_receiver.clone(),
                )
                .context("build_storage_caches()")?,
            );

            let started_at = Instant::now();
            tracing::info!("Initializing HTTP API");
            let bounded_gas_adjuster = gas_adjuster
                .get_or_init()
                .await
                .context("gas_adjuster.get_or_init()")?;
            let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
                bounded_gas_adjuster,
                FeeModelConfig::from_state_keeper_config(&state_keeper_config),
            ));
            run_http_api(
                &mut task_futures,
                &app_health,
                &database_secrets,
                &tx_sender_config,
                &state_keeper_config,
                &internal_api_config,
                &api_config,
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
                batch_fee_input_provider,
                state_keeper_config.save_call_traces,
                storage_caches.clone().unwrap(),
                mempool_cache.clone(),
            )
            .await
            .context("run_http_api")?;

            let elapsed = started_at.elapsed();
            APP_METRICS.init_latency[&InitStage::HttpApi].set(elapsed);
            tracing::info!(
                "Initialized HTTP API on port {:?} in {elapsed:?}",
                api_config.web3_json_rpc.http_port
            );
        }

        if components.contains(&Component::WsApi) {
            let storage_caches = match storage_caches {
                Some(storage_caches) => storage_caches,
                None => build_storage_caches(
                    &configs.api_config.clone().context("api")?.web3_json_rpc,
                    &replica_connection_pool,
                    &mut task_futures,
                    stop_receiver.clone(),
                )
                .context("build_storage_caches()")?,
            };

            let started_at = Instant::now();
            tracing::info!("initializing WS API");
            let bounded_gas_adjuster = gas_adjuster
                .get_or_init()
                .await
                .context("gas_adjuster.get_or_init()")?;
            let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
                bounded_gas_adjuster,
                FeeModelConfig::from_state_keeper_config(&state_keeper_config),
            ));
            run_ws_api(
                &mut task_futures,
                &app_health,
                &database_secrets,
                &tx_sender_config,
                &state_keeper_config,
                &internal_api_config,
                &api_config,
                batch_fee_input_provider,
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
                storage_caches,
                mempool_cache,
            )
            .await
            .context("run_ws_api")?;

            let elapsed = started_at.elapsed();
            APP_METRICS.init_latency[&InitStage::WsApi].set(elapsed);
            tracing::info!(
                "Initialized WS API on port {} in {elapsed:?}",
                api_config.web3_json_rpc.ws_port
            );
        }

        if components.contains(&Component::ContractVerificationApi) {
            let started_at = Instant::now();
            tracing::info!("initializing contract verification REST API");
            task_futures.push(tokio::spawn(
                zksync_contract_verification_server::start_server(
                    connection_pool.clone(),
                    replica_connection_pool.clone(),
                    configs
                        .contract_verifier
                        .clone()
                        .context("Contract verifier")?,
                    stop_receiver.clone(),
                ),
            ));
            let elapsed = started_at.elapsed();
            APP_METRICS.init_latency[&InitStage::ContractVerificationApi].set(elapsed);
            tracing::info!("initialized contract verification REST API in {elapsed:?}");
        }
    }

    let object_store_config = configs
        .core_object_store
        .clone()
        .context("core_object_store_config")?;
    let store_factory = ObjectStoreFactory::new(object_store_config);

    if components.contains(&Component::StateKeeper) {
        let started_at = Instant::now();
        tracing::info!("initializing State Keeper");
        let bounded_gas_adjuster = gas_adjuster
            .get_or_init()
            .await
            .context("gas_adjuster.get_or_init()")?;
        let state_keeper_config = configs
            .state_keeper_config
            .clone()
            .context("state_keeper_config")?;
        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            bounded_gas_adjuster,
            FeeModelConfig::from_state_keeper_config(&state_keeper_config),
        ));
        add_state_keeper_to_task_futures(
            &mut task_futures,
            &database_secrets,
            contracts_config,
            state_keeper_config,
            wallets
                .state_keeper
                .clone()
                .context("State keeper wallets")?,
            l2_chain_id,
            &db_config,
            &configs.mempool_config.clone().context("mempool_config")?,
            batch_fee_input_provider,
            stop_receiver.clone(),
        )
        .await
        .context("add_state_keeper_to_task_futures()")?;

        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::StateKeeper].set(elapsed);
        tracing::info!("initialized State Keeper in {elapsed:?}");
    }

    let diamond_proxy_addr = contracts_config.diamond_proxy_addr;
    let state_transition_manager_addr = contracts_config
        .ecosystem_contracts
        .as_ref()
        .map(|a| a.state_transition_proxy_addr);

    if components.contains(&Component::Consensus) {
        let cfg = consensus_config
            .clone()
            .context("consensus component's config is missing")?;
        let secrets = secrets
            .consensus
            .clone()
            .context("consensus component's secrets are missing")?;
        let started_at = Instant::now();
        tracing::info!("initializing Consensus");
        let pool = connection_pool.clone();
        let mut stop_receiver = stop_receiver.clone();
        task_futures.push(tokio::spawn(async move {
            // We instantiate the root context here, since the consensus task is the only user of the
            // structured concurrency framework.
            // Note, however, that awaiting for the `stop_receiver` is related to the root context behavior,
            // not the consensus task itself. There may have been any number of tasks running in the root context,
            // but we only need to wait for stop signal once, and it will be propagated to all child contexts.
            let root_ctx = ctx::root();
            scope::run!(&root_ctx, |ctx, s| async move {
                s.spawn_bg(zksync_node_consensus::era::run_main_node(
                    ctx, cfg, secrets, pool,
                ));
                let _ = stop_receiver.wait_for(|stop| *stop).await?;
                Ok(())
            })
            .await
        }));

        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::Consensus].set(elapsed);
        tracing::info!("initialized Consensus in {elapsed:?}");
    }

    if components.contains(&Component::EthWatcher) {
        let started_at = Instant::now();
        tracing::info!("initializing ETH-Watcher");
        let eth_watch_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
            .build()
            .await
            .context("failed to build eth_watch_pool")?;
        let governance = (governance_contract(), contracts_config.governance_addr);
        let eth_watch_config = configs
            .eth
            .clone()
            .context("eth_config")?
            .watcher
            .context("watcher")?;
        task_futures.push(
            start_eth_watch(
                eth_watch_config,
                eth_watch_pool,
                query_client.clone(),
                diamond_proxy_addr,
                state_transition_manager_addr,
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
        let eth_sender_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
            .build()
            .await
            .context("failed to build eth_sender_pool")?;

        let eth_sender_wallets = wallets.eth_sender.clone().context("eth_sender")?;
        let operator_private_key = eth_sender_wallets.operator.private_key();
        let diamond_proxy_addr = contracts_config.diamond_proxy_addr;
        let default_priority_fee_per_gas = eth
            .gas_adjuster
            .as_ref()
            .context("gas_adjuster")?
            .default_priority_fee_per_gas;
        let l1_chain_id = genesis_config.l1_chain_id;

        let eth_client = PKSigningClient::new_raw(
            operator_private_key.clone(),
            diamond_proxy_addr,
            default_priority_fee_per_gas,
            l1_chain_id,
            query_client.clone(),
        );

        let l1_batch_commit_data_generator_mode =
            genesis_config.l1_batch_commit_data_generator_mode;
        // Run the task synchronously: the main node is expected to have a stable Ethereum client connection,
        // and the cost of detecting an incorrect mode with a delay is higher.
        L1BatchCommitmentModeValidationTask::new(
            contracts_config.diamond_proxy_addr,
            l1_batch_commit_data_generator_mode,
            query_client.clone(),
        )
        .exit_on_success()
        .run(stop_receiver.clone())
        .await?;

        let operator_blobs_address = eth_sender_wallets.blob_operator.map(|x| x.address());

        let sender_config = eth.sender.clone().context("eth_sender")?;
        let eth_tx_aggregator_actor = EthTxAggregator::new(
            eth_sender_pool,
            sender_config.clone(),
            Aggregator::new(
                sender_config.clone(),
                store_factory.create_store().await?,
                operator_blobs_address.is_some(),
                l1_batch_commit_data_generator_mode,
            ),
            Box::new(eth_client),
            contracts_config.validator_timelock_addr,
            contracts_config.l1_multicall3_addr,
            diamond_proxy_addr,
            l2_chain_id,
            operator_blobs_address,
        )
        .await;
        task_futures.push(tokio::spawn(
            eth_tx_aggregator_actor.run(stop_receiver.clone()),
        ));
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::EthTxAggregator].set(elapsed);
        tracing::info!("initialized ETH-TxAggregator in {elapsed:?}");
    }

    if components.contains(&Component::EthTxManager) {
        let started_at = Instant::now();
        tracing::info!("initializing ETH-TxManager");
        let eth_manager_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
            .build()
            .await
            .context("failed to build eth_manager_pool")?;
        let eth_sender = configs.eth.clone().context("eth_sender_config")?;
        let eth_sender_wallets = wallets.eth_sender.clone().context("eth_sender")?;
        let operator_private_key = eth_sender_wallets.operator.private_key();
        let diamond_proxy_addr = contracts_config.diamond_proxy_addr;
        let default_priority_fee_per_gas = eth
            .gas_adjuster
            .as_ref()
            .context("gas_adjuster")?
            .default_priority_fee_per_gas;
        let l1_chain_id = genesis_config.l1_chain_id;

        let eth_client = PKSigningClient::new_raw(
            operator_private_key.clone(),
            diamond_proxy_addr,
            default_priority_fee_per_gas,
            l1_chain_id,
            query_client.clone(),
        );

        let eth_client_blobs = if let Some(blob_operator) = eth_sender_wallets.blob_operator {
            let operator_blob_private_key = blob_operator.private_key().clone();
            let client = Box::new(PKSigningClient::new_raw(
                operator_blob_private_key,
                diamond_proxy_addr,
                default_priority_fee_per_gas,
                l1_chain_id,
                query_client,
            ));
            Some(client as Box<dyn BoundEthInterface>)
        } else {
            None
        };

        let eth_tx_manager_actor = EthTxManager::new(
            eth_manager_pool,
            eth_sender.sender.clone().context("eth_sender")?,
            gas_adjuster
                .get_or_init()
                .await
                .context("gas_adjuster.get_or_init()")?,
            Box::new(eth_client),
            eth_client_blobs,
        );
        task_futures.extend([tokio::spawn(
            eth_tx_manager_actor.run(stop_receiver.clone()),
        )]);
        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::EthTxManager].set(elapsed);
        tracing::info!("initialized ETH-TxManager in {elapsed:?}");
    }

    add_trees_to_task_futures(
        configs,
        secrets,
        &mut task_futures,
        &app_health,
        components,
        &store_factory,
        stop_receiver.clone(),
    )
    .await
    .context("add_trees_to_task_futures()")?;

    if components.contains(&Component::TeeVerifierInputProducer) {
        let singleton_connection_pool =
            ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
                .build()
                .await
                .context("failed to build singleton connection_pool")?;
        add_tee_verifier_input_producer_to_task_futures(
            &mut task_futures,
            &singleton_connection_pool,
            &store_factory,
            l2_chain_id,
            stop_receiver.clone(),
        )
        .await
        .context("add_tee_verifier_input_producer_to_task_futures()")?;
    }

    if components.contains(&Component::Housekeeper) {
        add_house_keeper_to_task_futures(
            configs,
            secrets,
            &mut task_futures,
            stop_receiver.clone(),
        )
        .await
        .context("add_house_keeper_to_task_futures()")?;
    }

    if components.contains(&Component::ProofDataHandler) {
        task_futures.push(tokio::spawn(zksync_proof_data_handler::run_server(
            configs
                .proof_data_handler_config
                .clone()
                .context("proof_data_handler_config")?,
            store_factory.create_store().await?,
            connection_pool.clone(),
            genesis_config.l1_batch_commit_data_generator_mode,
            stop_receiver.clone(),
        )));
    }

    if components.contains(&Component::CommitmentGenerator) {
        let pool_size = CommitmentGenerator::default_parallelism().get();
        let commitment_generator_pool =
            ConnectionPool::<Core>::builder(database_secrets.master_url()?, pool_size)
                .build()
                .await
                .context("failed to build commitment_generator_pool")?;
        let commitment_generator = CommitmentGenerator::new(
            commitment_generator_pool,
            genesis_config.l1_batch_commit_data_generator_mode,
        );
        app_health.insert_component(commitment_generator.health_check())?;
        task_futures.push(tokio::spawn(
            commitment_generator.run(stop_receiver.clone()),
        ));
    }

    // Run healthcheck server for all components.
    let db_health_check = ConnectionPoolHealthCheck::new(replica_connection_pool);
    app_health.insert_custom_component(Arc::new(db_health_check))?;
    let health_check_handle =
        HealthCheckHandle::spawn_server(health_check_config.bind_addr(), app_health);

    if let Some(task) = gas_adjuster.run_if_initialized(stop_receiver.clone()) {
        task_futures.push(task);
    }

    Ok((task_futures, stop_sender, health_check_handle))
}

#[allow(clippy::too_many_arguments)]
async fn add_state_keeper_to_task_futures(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    database_secrets: &DatabaseSecrets,
    contracts_config: &ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    state_keeper_wallets: wallets::StateKeeper,
    l2chain_id: L2ChainId,
    db_config: &DBConfig,
    mempool_config: &MempoolConfig,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let state_keeper_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
        .build()
        .await
        .context("failed to build state_keeper_pool")?;
    let mempool = {
        let mut storage = state_keeper_pool
            .connection()
            .await
            .context("Access storage to build mempool")?;
        let mempool = MempoolGuard::from_storage(&mut storage, mempool_config.capacity).await;
        mempool.register_metrics();
        mempool
    };

    // L2 Block sealing process is parallelized, so we have to provide enough pooled connections.
    let persistence_pool = ConnectionPool::<Core>::builder(
        database_secrets.master_url()?,
        L2BlockSealProcess::subtasks_len(),
    )
    .build()
    .await
    .context("failed to build l2_block_sealer_pool")?;
    let (persistence, l2_block_sealer) = StateKeeperPersistence::new(
        persistence_pool.clone(),
        contracts_config
            .l2_shared_bridge_addr
            .context("`l2_shared_bridge_addr` config is missing")?,
        state_keeper_config.l2_block_seal_queue_capacity,
    );
    task_futures.push(tokio::spawn(l2_block_sealer.run()));

    // One (potentially held long-term) connection for `AsyncCatchupTask` and another connection
    // to access `AsyncRocksdbCache` as a storage.
    let async_cache_pool = ConnectionPool::<Core>::builder(database_secrets.master_url()?, 2)
        .build()
        .await
        .context("failed to build async_cache_pool")?;
    let cache_options = RocksdbStorageOptions {
        block_cache_capacity: db_config
            .experimental
            .state_keeper_db_block_cache_capacity(),
        max_open_files: db_config.experimental.state_keeper_db_max_open_files,
    };
    let (async_cache, async_catchup_task) = AsyncRocksdbCache::new(
        async_cache_pool,
        db_config.state_keeper_db_path.clone(),
        cache_options,
    );

    let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
    let output_handler =
        OutputHandler::new(Box::new(persistence)).with_handler(Box::new(tree_writes_persistence));
    let state_keeper = create_state_keeper(
        state_keeper_config,
        state_keeper_wallets,
        async_cache,
        l2chain_id,
        mempool_config,
        state_keeper_pool,
        mempool.clone(),
        batch_fee_input_provider.clone(),
        output_handler,
        stop_receiver.clone(),
    )
    .await;

    let mut stop_receiver_clone = stop_receiver.clone();
    task_futures.push(tokio::task::spawn(async move {
        let result = async_catchup_task.run(stop_receiver_clone.clone()).await;
        stop_receiver_clone.changed().await?;
        result
    }));
    task_futures.push(tokio::spawn(state_keeper.run()));

    let mempool_fetcher_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
        .build()
        .await
        .context("failed to build mempool_fetcher_pool")?;
    let mempool_fetcher = MempoolFetcher::new(
        mempool,
        batch_fee_input_provider,
        mempool_config,
        mempool_fetcher_pool,
    );
    let mempool_fetcher_handle = tokio::spawn(mempool_fetcher.run(stop_receiver));
    task_futures.push(mempool_fetcher_handle);
    Ok(())
}

pub async fn start_eth_watch(
    config: EthWatchConfig,
    pool: ConnectionPool<Core>,
    eth_gateway: Box<DynClient<L1>>,
    diamond_proxy_addr: Address,
    state_transition_manager_addr: Option<Address>,
    governance: (Contract, Address),
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let eth_client = EthHttpQueryClient::new(
        eth_gateway,
        diamond_proxy_addr,
        state_transition_manager_addr,
        governance.1,
        config.confirmations_for_eth_event,
    );

    let eth_watch = EthWatch::new(
        diamond_proxy_addr,
        &governance.0,
        Box::new(eth_client),
        pool,
        config.poll_interval(),
    )
    .await?;

    Ok(tokio::spawn(eth_watch.run(stop_receiver)))
}

async fn add_trees_to_task_futures(
    configs: &GeneralConfig,
    secrets: &Secrets,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    components: &[Component],
    store_factory: &ObjectStoreFactory,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    if !components.contains(&Component::Tree) {
        anyhow::ensure!(
            !components.contains(&Component::TreeApi),
            "Merkle tree API cannot be started without a tree component"
        );
        return Ok(());
    }

    let db_config = configs.db_config.clone().context("db_config")?;
    let database_secrets = secrets.database.clone().context("database_secrets")?;
    let operation_config = configs
        .operations_manager_config
        .clone()
        .context("operations_manager_config")?;
    let api_config = configs
        .api_config
        .clone()
        .context("api_config")?
        .merkle_tree;
    let api_config = components
        .contains(&Component::TreeApi)
        .then_some(&api_config);

    let object_store = match db_config.merkle_tree.mode {
        MerkleTreeMode::Lightweight => None,
        MerkleTreeMode::Full => Some(store_factory.create_store().await?),
    };

    run_tree(
        task_futures,
        app_health,
        &database_secrets,
        &db_config.merkle_tree,
        api_config,
        &operation_config,
        object_store,
        stop_receiver,
    )
    .await
    .context("run_tree()")
}

#[allow(clippy::too_many_arguments)]
async fn run_tree(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    database_secrets: &DatabaseSecrets,
    merkle_tree_config: &MerkleTreeConfig,
    api_config: Option<&MerkleTreeApiConfig>,
    operation_manager: &OperationsManagerConfig,
    object_store: Option<Arc<dyn ObjectStore>>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let started_at = Instant::now();
    let mode_str = if matches!(merkle_tree_config.mode, MerkleTreeMode::Full) {
        "full"
    } else {
        "lightweight"
    };
    tracing::info!("Initializing Merkle tree in {mode_str} mode");

    let config = MetadataCalculatorConfig::for_main_node(merkle_tree_config, operation_manager);
    let pool = ConnectionPool::singleton(database_secrets.master_url()?)
        .build()
        .await
        .context("failed to build connection pool for Merkle tree")?;
    // The number of connections in a recovery pool is based on the mainnet recovery runs. It doesn't need
    // to be particularly accurate at this point, since the main node isn't expected to recover from a snapshot.
    let recovery_pool = ConnectionPool::builder(database_secrets.replica_url()?, 10)
        .build()
        .await
        .context("failed to build connection pool for Merkle tree recovery")?;
    let metadata_calculator = MetadataCalculator::new(config, object_store, pool)
        .await
        .context("failed initializing metadata_calculator")?
        .with_recovery_pool(recovery_pool);

    if let Some(api_config) = api_config {
        let address = (Ipv4Addr::UNSPECIFIED, api_config.port).into();
        let tree_reader = metadata_calculator.tree_reader();
        let stop_receiver = stop_receiver.clone();
        task_futures.push(tokio::spawn(async move {
            tree_reader
                .wait()
                .await
                .context("Cannot initialize tree reader")?
                .run_api_server(address, stop_receiver)
                .await
        }));
    }

    let tree_health_check = metadata_calculator.tree_health_check();
    app_health.insert_custom_component(Arc::new(tree_health_check))?;
    let tree_task = tokio::spawn(metadata_calculator.run(stop_receiver));
    task_futures.push(tree_task);

    let elapsed = started_at.elapsed();
    APP_METRICS.init_latency[&InitStage::Tree].set(elapsed);
    tracing::info!("Initialized {mode_str} tree in {elapsed:?}");
    Ok(())
}

async fn add_tee_verifier_input_producer_to_task_futures(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    connection_pool: &ConnectionPool<Core>,
    store_factory: &ObjectStoreFactory,
    l2_chain_id: L2ChainId,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    /*
    let started_at = Instant::now();
    tracing::info!("initializing TeeVerifierInputProducer");
    let producer = TeeVerifierInputProducer::new(
        connection_pool.clone(),
        store_factory.create_store().await?,
        l2_chain_id,
    )
    .await?;
    task_futures.push(tokio::spawn(producer.run(stop_receiver, None)));
    tracing::info!(
        "Initialized TeeVerifierInputProducer in {:?}",
        started_at.elapsed()
    );
    let elapsed = started_at.elapsed();
    APP_METRICS.init_latency[&InitStage::TeeVerifierInputProducer].set(elapsed);
    */
    Ok(())
}

async fn add_house_keeper_to_task_futures(
    configs: &GeneralConfig,
    secrets: &Secrets,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let house_keeper_config = configs
        .house_keeper_config
        .clone()
        .context("house_keeper_config")?;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;
    let secrets = secrets.database.clone().context("database_secrets")?;
    let connection_pool =
        ConnectionPool::<Core>::builder(secrets.replica_url()?, postgres_config.max_connections()?)
            .build()
            .await
            .context("failed to build a connection pool")?;

    let pool_for_metrics = connection_pool.clone();
    let mut stop_receiver_for_metrics = stop_receiver.clone();
    task_futures.push(tokio::spawn(async move {
        tokio::select! {
            () = PostgresMetrics::run_scraping(pool_for_metrics, Duration::from_secs(60)) => {
                tracing::warn!("Postgres metrics scraping unexpectedly stopped");
            }
            _ = stop_receiver_for_metrics.changed() => {
                tracing::info!("Stop signal received, Postgres metrics scraping is shutting down");
            }
        }
        Ok(())
    }));

    let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
        house_keeper_config.l1_batch_metrics_reporting_interval_ms,
        connection_pool.clone(),
    );

    let prover_connection_pool = ConnectionPool::<Prover>::builder(
        secrets.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a prover_connection_pool")?;
    let task = l1_batch_metrics_reporter.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    // All FRI Prover related components are configured below.
    let fri_prover_config = configs.prover_config.clone().context("fri_prover_config")?;
    let fri_prover_job_retry_manager = FriProverJobRetryManager::new(
        fri_prover_config.max_attempts,
        fri_prover_config.proof_generation_timeout(),
        house_keeper_config.prover_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    let task = fri_prover_job_retry_manager.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    let fri_witness_gen_config = configs
        .witness_generator
        .clone()
        .context("fri_witness_generator_config")?;
    let fri_witness_gen_job_retry_manager = FriWitnessGeneratorJobRetryManager::new(
        fri_witness_gen_config.max_attempts,
        fri_witness_gen_config.witness_generation_timeouts(),
        house_keeper_config.witness_generator_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    let task = fri_witness_gen_job_retry_manager.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    let waiting_to_queued_fri_witness_job_mover = WaitingToQueuedFriWitnessJobMover::new(
        house_keeper_config.witness_job_moving_interval_ms,
        prover_connection_pool.clone(),
    );
    let task = waiting_to_queued_fri_witness_job_mover.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    let fri_witness_generator_stats_reporter = FriWitnessGeneratorQueueReporter::new(
        prover_connection_pool.clone(),
        house_keeper_config.witness_generator_stats_reporting_interval_ms,
    );
    let task = fri_witness_generator_stats_reporter.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    // TODO(PLA-862): remove after fields become required
    if let Some((archiving_interval, archive_after)) =
        house_keeper_config.prover_job_archiver_params()
    {
        let fri_prover_jobs_archiver = FriProverJobsArchiver::new(
            prover_connection_pool.clone(),
            archiving_interval,
            archive_after,
        );
        let task = fri_prover_jobs_archiver.run(stop_receiver.clone());
        task_futures.push(tokio::spawn(task));
    }

    if let Some((archiving_interval, archive_after)) =
        house_keeper_config.fri_gpu_prover_archiver_params()
    {
        let fri_gpu_prover_jobs_archiver = FriGpuProverArchiver::new(
            prover_connection_pool.clone(),
            archiving_interval,
            archive_after,
        );
        let task = fri_gpu_prover_jobs_archiver.run(stop_receiver.clone());
        task_futures.push(tokio::spawn(task));
    }

    let fri_prover_group_config = configs
        .prover_group_config
        .clone()
        .context("fri_prover_group_config")?;
    let fri_prover_stats_reporter = FriProverQueueReporter::new(
        house_keeper_config.prover_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
        connection_pool.clone(),
        fri_prover_group_config,
    );
    let task = fri_prover_stats_reporter.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    let proof_compressor_config = configs
        .proof_compressor_config
        .clone()
        .context("fri_proof_compressor_config")?;
    let fri_proof_compressor_stats_reporter = FriProofCompressorQueueReporter::new(
        house_keeper_config.proof_compressor_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
    );
    let task = fri_proof_compressor_stats_reporter.run(stop_receiver.clone());
    task_futures.push(tokio::spawn(task));

    let fri_proof_compressor_retry_manager = FriProofCompressorJobRetryManager::new(
        proof_compressor_config.max_attempts,
        proof_compressor_config.generation_timeout(),
        house_keeper_config.proof_compressor_job_retrying_interval_ms,
        prover_connection_pool.clone(),
    );
    let task = fri_proof_compressor_retry_manager.run(stop_receiver);
    task_futures.push(tokio::spawn(task));
    Ok(())
}

fn build_storage_caches(
    rpc_config: &Web3JsonRpcConfig,
    replica_connection_pool: &ConnectionPool<Core>,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<PostgresStorageCaches> {
    let factory_deps_capacity = rpc_config.factory_deps_cache_size() as u64;
    let initial_writes_capacity = rpc_config.initial_writes_cache_size() as u64;
    let values_capacity = rpc_config.latest_values_cache_size() as u64;
    let mut storage_caches =
        PostgresStorageCaches::new(factory_deps_capacity, initial_writes_capacity);

    if values_capacity > 0 {
        let values_cache_task = storage_caches
            .configure_storage_values_cache(values_capacity, replica_connection_pool.clone());
        task_futures.push(tokio::task::spawn(values_cache_task.run(stop_receiver)));
    }
    Ok(storage_caches)
}

#[allow(clippy::too_many_arguments)]
async fn run_http_api(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    database_secrets: &DatabaseSecrets,
    tx_sender_config: &TxSenderConfig,
    state_keeper_config: &StateKeeperConfig,
    internal_api: &InternalApiConfig,
    api_config: &ApiConfig,
    master_connection_pool: ConnectionPool<Core>,
    replica_connection_pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
    batch_fee_model_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    with_debug_namespace: bool,
    storage_caches: PostgresStorageCaches,
    mempool_cache: MempoolCache,
) -> anyhow::Result<()> {
    let (tx_sender, vm_barrier) = build_tx_sender(
        tx_sender_config,
        &api_config.web3_json_rpc,
        state_keeper_config,
        replica_connection_pool.clone(),
        master_connection_pool,
        batch_fee_model_input_provider,
        storage_caches,
    )
    .await?;

    let mut namespaces = Namespace::DEFAULT.to_vec();
    if with_debug_namespace {
        namespaces.push(Namespace::Debug)
    }
    namespaces.push(Namespace::Snapshots);

    let updaters_pool = ConnectionPool::<Core>::builder(database_secrets.replica_url()?, 2)
        .build()
        .await
        .context("failed to build updaters_pool")?;

    let mut api_builder =
        web3::ApiBuilder::jsonrpsee_backend(internal_api.clone(), replica_connection_pool)
            .http(api_config.web3_json_rpc.http_port)
            .with_updaters_pool(updaters_pool)
            .with_filter_limit(api_config.web3_json_rpc.filters_limit())
            .with_batch_request_size_limit(api_config.web3_json_rpc.max_batch_request_size())
            .with_response_body_size_limit(api_config.web3_json_rpc.max_response_body_size())
            .with_tx_sender(tx_sender)
            .with_vm_barrier(vm_barrier)
            .with_mempool_cache(mempool_cache)
            .enable_api_namespaces(namespaces);
    if let Some(tree_api_url) = api_config.web3_json_rpc.tree_api_url() {
        let tree_api = Arc::new(TreeApiHttpClient::new(tree_api_url));
        api_builder = api_builder.with_tree_api(tree_api.clone());
        app_health.insert_custom_component(tree_api)?;
    }

    let server_handles = api_builder
        .build()
        .context("failed to build HTTP API server")?
        .run(stop_receiver)
        .await?;
    task_futures.extend(server_handles.tasks);
    app_health.insert_component(server_handles.health_check)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_ws_api(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    database_secrets: &DatabaseSecrets,
    tx_sender_config: &TxSenderConfig,
    state_keeper_config: &StateKeeperConfig,
    internal_api: &InternalApiConfig,
    api_config: &ApiConfig,
    batch_fee_model_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    master_connection_pool: ConnectionPool<Core>,
    replica_connection_pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
    storage_caches: PostgresStorageCaches,
    mempool_cache: MempoolCache,
) -> anyhow::Result<()> {
    let (tx_sender, vm_barrier) = build_tx_sender(
        tx_sender_config,
        &api_config.web3_json_rpc,
        state_keeper_config,
        replica_connection_pool.clone(),
        master_connection_pool,
        batch_fee_model_input_provider,
        storage_caches,
    )
    .await?;
    let updaters_pool = ConnectionPool::<Core>::singleton(database_secrets.replica_url()?)
        .build()
        .await
        .context("failed to build updaters_pool")?;

    let mut namespaces = Namespace::DEFAULT.to_vec();
    namespaces.push(Namespace::Snapshots);

    let mut api_builder =
        web3::ApiBuilder::jsonrpsee_backend(internal_api.clone(), replica_connection_pool)
            .ws(api_config.web3_json_rpc.ws_port)
            .with_updaters_pool(updaters_pool)
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
            .with_tx_sender(tx_sender)
            .with_vm_barrier(vm_barrier)
            .with_mempool_cache(mempool_cache)
            .enable_api_namespaces(namespaces);
    if let Some(tree_api_url) = api_config.web3_json_rpc.tree_api_url() {
        let tree_api = Arc::new(TreeApiHttpClient::new(tree_api_url));
        api_builder = api_builder.with_tree_api(tree_api.clone());
        app_health.insert_custom_component(tree_api)?;
    }

    let server_handles = api_builder
        .build()
        .context("failed to build WS API server")?
        .run(stop_receiver)
        .await?;
    task_futures.extend(server_handles.tasks);
    app_health.insert_component(server_handles.health_check)?;
    Ok(())
}

async fn circuit_breakers_for_components(
    components: &[Component],
    database_secrets: &DatabaseSecrets,
    circuit_breaker_config: &CircuitBreakerConfig,
) -> anyhow::Result<CircuitBreakers> {
    let circuit_breakers = CircuitBreakers::default();

    if components
        .iter()
        .any(|c| matches!(c, Component::EthTxAggregator | Component::EthTxManager))
    {
        let pool = ConnectionPool::<Core>::singleton(database_secrets.replica_url()?)
            .build()
            .await
            .context("failed to build a connection pool")?;
        circuit_breakers
            .insert(Box::new(FailedL1TransactionChecker { pool }))
            .await;
    }

    if components.iter().any(|c| {
        matches!(
            c,
            Component::HttpApi | Component::WsApi | Component::ContractVerificationApi
        )
    }) {
        let pool = ConnectionPool::<Core>::singleton(database_secrets.replica_url()?)
            .build()
            .await?;
        circuit_breakers
            .insert(Box::new(ReplicationLagChecker {
                pool,
                replication_lag_limit: circuit_breaker_config.replication_lag_limit(),
            }))
            .await;
    }
    Ok(circuit_breakers)
}
