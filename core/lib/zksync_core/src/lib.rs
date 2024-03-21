#![allow(clippy::upper_case_acronyms, clippy::derive_partial_eq_without_eq)]

use std::{
    net::Ipv4Addr,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use api_server::tx_sender::master_pool_sink::MasterPoolSink;
use fee_model::{ApiFeeInputProvider, BatchFeeModelInputProvider, MainNodeFeeInputProvider};
use futures::channel::oneshot;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::Prover;
use temp_config_store::{Secrets, TempConfigStore};
use tokio::{sync::watch, task::JoinHandle};
use zksync_circuit_breaker::{
    l1_txs::FailedL1TransactionChecker, replication_lag::ReplicationLagChecker, CircuitBreaker,
    CircuitBreakerChecker, CircuitBreakerError,
};
use zksync_concurrency::{ctx, scope};
use zksync_config::{
    configs::{
        api::{MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        contracts::ProverAtGenesis,
        database::{MerkleTreeConfig, MerkleTreeMode},
    },
    ApiConfig, ContractsConfig, DBConfig, ETHSenderConfig, PostgresConfig,
};
use zksync_contracts::{governance_contract, BaseSystemContracts};
use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core, CoreDal};
use zksync_db_connection::healthcheck::ConnectionPoolHealthCheck;
use zksync_eth_client::{
    clients::{PKSigningClient, QueryClient},
    BoundEthInterface, CallFunctionArgs, EthInterface,
};
use zksync_health_check::{AppHealthCheck, HealthStatus, ReactiveHealthCheck};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    fee_model::FeeModelConfig,
    protocol_version::{L1VerifierConfig, VerifierParams},
    system_contracts::get_system_smart_contracts,
    web3::contract::tokens::Detokenize,
    L2ChainId, PackedEthSignature, ProtocolVersionId,
};

use crate::{
    api_server::{
        contract_verification,
        execution_sandbox::{VmConcurrencyBarrier, VmConcurrencyLimiter},
        healthcheck::HealthCheckHandle,
        tree::TreeApiHttpClient,
        tx_sender::{ApiContracts, TxSender, TxSenderBuilder, TxSenderConfig},
        web3::{self, state::InternalApiConfig, Namespace},
    },
    basic_witness_input_producer::BasicWitnessInputProducer,
    commitment_generator::CommitmentGenerator,
    eth_sender::{Aggregator, EthTxAggregator, EthTxManager},
    eth_watch::start_eth_watch,
    house_keeper::{
        blocks_state_reporter::L1BatchMetricsReporter,
        fri_proof_compressor_job_retry_manager::FriProofCompressorJobRetryManager,
        fri_proof_compressor_queue_monitor::FriProofCompressorStatsReporter,
        fri_prover_job_retry_manager::FriProverJobRetryManager,
        fri_prover_queue_monitor::FriProverStatsReporter,
        fri_scheduler_circuit_queuer::SchedulerCircuitQueuer,
        fri_witness_generator_jobs_retry_manager::FriWitnessGeneratorJobRetryManager,
        fri_witness_generator_queue_monitor::FriWitnessGeneratorStatsReporter,
        periodic_job::PeriodicJob,
        waiting_to_queued_fri_witness_job_mover::WaitingToQueuedFriWitnessJobMover,
    },
    l1_gas_price::GasAdjusterSingleton,
    metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig},
    metrics::{InitStage, APP_METRICS},
    state_keeper::{
        create_state_keeper, MempoolFetcher, MempoolGuard, MiniblockSealer, SequencerSealer,
    },
};

pub mod api_server;
pub mod basic_witness_input_producer;
pub mod block_reverter;
pub mod commitment_generator;
pub mod consensus;
pub mod consistency_checker;
pub mod db_pruner;
pub mod eth_sender;
pub mod eth_watch;
pub mod fee_model;
pub mod gas_tracker;
pub mod genesis;
pub mod house_keeper;
pub mod l1_gas_price;
pub mod metadata_calculator;
mod metrics;
pub mod proof_data_handler;
pub mod proto;
pub mod reorg_detector;
pub mod state_keeper;
pub mod sync_layer;
pub mod temp_config_store;
mod utils;

/// Inserts the initial information about zkSync tokens into the database.
pub async fn genesis_init(
    postgres_config: &PostgresConfig,
    eth_sender: &ETHSenderConfig,
    network_config: &NetworkConfig,
    contracts_config: &ContractsConfig,
    eth_client_url: &str,
    wait_for_set_chain_id: bool,
) -> anyhow::Result<()> {
    let db_url = postgres_config.master_url()?;
    let pool = ConnectionPool::<Core>::singleton(db_url)
        .build()
        .await
        .context("failed to build connection_pool")?;
    let mut storage = pool.connection().await.context("connection()")?;
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
            let args = CallFunctionArgs::new("verificationKeyHash", ()).for_contract(
                contracts_config.verifier_addr,
                zksync_contracts::verifier_contract(),
            );

            let vk_hash = eth_client.call_contract_function(args).await?;
            let vk_hash = zksync_types::H256::from_tokens(vk_hash)?;

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
            first_l1_verifier_config,
        },
    )
    .await?;

    if wait_for_set_chain_id {
        genesis::save_set_chain_id_tx(
            eth_client_url,
            contracts_config.diamond_proxy_addr,
            contracts_config
                .state_transition_proxy_addr
                .context("state_transition_proxy_addr is not set, but needed for genesis")?,
            &mut storage,
        )
        .await
        .context("Failed to save SetChainId upgrade transaction")?;
    }

    Ok(())
}

pub async fn is_genesis_needed(postgres_config: &PostgresConfig) -> bool {
    let db_url = postgres_config.master_url().unwrap();
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
    /// Produces input for basic witness generator and uploads it as bin encoded file (blob) to GCS.
    /// The blob is later used as input for Basic Witness Generators.
    BasicWitnessInputProducer,
    /// Component for housekeeping task such as cleaning blobs from GCS, reporting metrics etc.
    Housekeeper,
    /// Component for exposing APIs to prover for providing proof generation data and accepting proofs.
    ProofDataHandler,
    /// Component generating BFT consensus certificates for miniblocks.
    Consensus,
    /// Component generating commitment for L1 batches.
    CommitmentGenerator,
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
            "basic_witness_input_producer" => {
                Ok(Components(vec![Component::BasicWitnessInputProducer]))
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
            other => Err(format!("{} is not a valid component name", other)),
        }
    }
}

pub async fn initialize_components(
    configs: &TempConfigStore,
    components: Vec<Component>,
    secrets: &Secrets,
) -> anyhow::Result<(
    Vec<JoinHandle<anyhow::Result<()>>>,
    watch::Sender<bool>,
    oneshot::Receiver<CircuitBreakerError>,
    HealthCheckHandle,
)> {
    tracing::info!("Starting the components: {components:?}");

    let db_config = configs.db_config.clone().context("db_config")?;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;

    if let Some(threshold) = postgres_config.slow_query_threshold() {
        ConnectionPool::<Core>::global_config().set_slow_query_threshold(threshold)?;
    }
    if let Some(threshold) = postgres_config.long_connection_threshold() {
        ConnectionPool::<Core>::global_config().set_long_connection_threshold(threshold)?;
    }

    let pool_size = postgres_config.max_connections()?;
    let connection_pool = ConnectionPool::<Core>::builder(postgres_config.master_url()?, pool_size)
        .build()
        .await
        .context("failed to build connection_pool")?;
    // We're most interested in setting acquire / statement timeouts for the API server, which puts the most load
    // on Postgres.
    let replica_connection_pool =
        ConnectionPool::<Core>::builder(postgres_config.replica_url()?, pool_size)
            .set_acquire_timeout(postgres_config.acquire_timeout())
            .set_statement_timeout(postgres_config.statement_timeout())
            .build()
            .await
            .context("failed to build replica_connection_pool")?;

    let health_check_config = configs
        .health_check_config
        .clone()
        .context("health_check_config")?;
    let app_health = Arc::new(AppHealthCheck::new(
        health_check_config.slow_time_limit(),
        health_check_config.hard_time_limit(),
    ));

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

    let eth_sender_config = configs
        .eth_sender_config
        .clone()
        .context("eth_sender_config")?;
    let mut gas_adjuster = GasAdjusterSingleton::new(
        eth_client_config.web3_url.clone(),
        gas_adjuster_config,
        eth_sender_config.sender.pubdata_sending_mode,
    );

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (cb_sender, cb_receiver) = oneshot::channel();

    // Prometheus exporter and circuit breaker checker should run for every component configuration.
    let prom_config = configs
        .prometheus_config
        .clone()
        .context("prometheus_config")?;
    let prom_config = PrometheusExporterConfig::pull(prom_config.listener_port);

    let (prometheus_health_check, prometheus_health_updater) =
        ReactiveHealthCheck::new("prometheus_exporter");
    app_health.insert_component(prometheus_health_check);
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
                build_storage_caches(
                    configs,
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
                &postgres_config,
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
                    configs,
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
                &postgres_config,
                &tx_sender_config,
                &state_keeper_config,
                &internal_api_config,
                &api_config,
                batch_fee_input_provider,
                connection_pool.clone(),
                replica_connection_pool.clone(),
                stop_receiver.clone(),
                storage_caches,
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
            &postgres_config,
            &contracts_config,
            state_keeper_config,
            &configs.network_config.clone().context("network_config")?,
            &db_config,
            &configs.mempool_config.clone().context("mempool_config")?,
            batch_fee_input_provider,
            store_factory.create_store().await,
            stop_receiver.clone(),
        )
        .await
        .context("add_state_keeper_to_task_futures()")?;

        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::StateKeeper].set(elapsed);
        tracing::info!("initialized State Keeper in {elapsed:?}");
    }

    if components.contains(&Component::Consensus) {
        let cfg = configs
            .consensus_config
            .as_ref()
            .context("consensus component's config is missing")?
            .main_node(
                secrets
                    .consensus
                    .as_ref()
                    .context("consensus secrets are missing")?,
            )?;
        let started_at = Instant::now();
        tracing::info!("initializing Consensus");
        let pool = connection_pool.clone();
        let mut stop_receiver = stop_receiver.clone();
        task_futures.push(tokio::spawn(async move {
            scope::run!(&ctx::root(), |ctx, s| async {
                s.spawn_bg(async {
                    // Consensus is a new component.
                    // For now in case of error we just log it and allow the server
                    // to continue running.
                    if let Err(err) = cfg.run(ctx, consensus::Store(pool)).await {
                        tracing::error!(%err, "Consensus actor failed");
                    } else {
                        tracing::info!("Consensus actor stopped");
                    }
                    Ok(())
                });
                let _ = stop_receiver.wait_for(|stop| *stop).await?;
                Ok(())
            })
            .await
        }));

        let elapsed = started_at.elapsed();
        APP_METRICS.init_latency[&InitStage::Consensus].set(elapsed);
        tracing::info!("initialized Consensus in {elapsed:?}");
    }

    let main_zksync_contract_address = contracts_config.diamond_proxy_addr;

    if components.contains(&Component::EthWatcher) {
        let started_at = Instant::now();
        tracing::info!("initializing ETH-Watcher");
        let eth_watch_pool = ConnectionPool::<Core>::singleton(postgres_config.master_url()?)
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
                Arc::new(query_client.clone()),
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
        let eth_sender_pool = ConnectionPool::<Core>::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build eth_sender_pool")?;

        let eth_sender = configs
            .eth_sender_config
            .clone()
            .context("eth_sender_config")?;
        let eth_client =
            PKSigningClient::from_config(&eth_sender, &contracts_config, &eth_client_config);
        let eth_client_blobs_addr =
            PKSigningClient::from_config_blobs(&eth_sender, &contracts_config, &eth_client_config)
                .map(|k| k.sender_account());

        let eth_tx_aggregator_actor = EthTxAggregator::new(
            eth_sender_pool,
            eth_sender.sender.clone(),
            Aggregator::new(
                eth_sender.sender.clone(),
                store_factory.create_store().await,
                eth_client_blobs_addr.is_some(),
                eth_sender.sender.pubdata_sending_mode.into(),
            ),
            Arc::new(eth_client),
            contracts_config.validator_timelock_addr,
            contracts_config.l1_multicall3_addr,
            main_zksync_contract_address,
            configs
                .network_config
                .as_ref()
                .context("network_config")?
                .zksync_network_id,
            eth_client_blobs_addr,
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
        let eth_manager_pool = ConnectionPool::<Core>::singleton(postgres_config.master_url()?)
            .build()
            .await
            .context("failed to build eth_manager_pool")?;
        let eth_sender = configs
            .eth_sender_config
            .clone()
            .context("eth_sender_config")?;
        let eth_client =
            PKSigningClient::from_config(&eth_sender, &contracts_config, &eth_client_config);
        let eth_client_blobs =
            PKSigningClient::from_config_blobs(&eth_sender, &contracts_config, &eth_client_config);
        let eth_tx_manager_actor = EthTxManager::new(
            eth_manager_pool,
            eth_sender.sender,
            gas_adjuster
                .get_or_init()
                .await
                .context("gas_adjuster.get_or_init()")?,
            Arc::new(eth_client),
            eth_client_blobs.map(|c| Arc::new(c) as Arc<dyn BoundEthInterface>),
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
        &mut task_futures,
        &app_health,
        &components,
        &store_factory,
        stop_receiver.clone(),
    )
    .await
    .context("add_trees_to_task_futures()")?;

    if components.contains(&Component::BasicWitnessInputProducer) {
        let singleton_connection_pool =
            ConnectionPool::<Core>::singleton(postgres_config.master_url()?)
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

    if components.contains(&Component::CommitmentGenerator) {
        let commitment_generator_pool =
            ConnectionPool::<Core>::singleton(postgres_config.master_url()?)
                .build()
                .await
                .context("failed to build commitment_generator_pool")?;
        let commitment_generator = CommitmentGenerator::new(commitment_generator_pool);
        app_health.insert_component(commitment_generator.health_check());
        task_futures.push(tokio::spawn(
            commitment_generator.run(stop_receiver.clone()),
        ));
    }

    // Run healthcheck server for all components.
    let db_health_check = ConnectionPoolHealthCheck::new(replica_connection_pool);
    app_health.insert_custom_component(Arc::new(db_health_check));
    let health_check_handle =
        HealthCheckHandle::spawn_server(health_check_config.bind_addr(), app_health);

    if let Some(task) = gas_adjuster.run_if_initialized(stop_receiver.clone()) {
        task_futures.push(task);
    }
    Ok((task_futures, stop_sender, cb_receiver, health_check_handle))
}

#[allow(clippy::too_many_arguments)]
async fn add_state_keeper_to_task_futures(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    postgres_config: &PostgresConfig,
    contracts_config: &ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    network_config: &NetworkConfig,
    db_config: &DBConfig,
    mempool_config: &MempoolConfig,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    object_store: Arc<dyn ObjectStore>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let pool_builder = ConnectionPool::<Core>::singleton(postgres_config.master_url()?);
    let state_keeper_pool = pool_builder
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
        state_keeper_pool.clone(),
        mempool.clone(),
        batch_fee_input_provider.clone(),
        miniblock_sealer_handle,
        object_store,
        stop_receiver.clone(),
    )
    .await;

    task_futures.push(tokio::spawn(
        state_keeper.run_fee_address_migration(state_keeper_pool),
    ));
    task_futures.push(tokio::spawn(state_keeper.run()));

    let mempool_fetcher_pool = pool_builder
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

async fn add_trees_to_task_futures(
    configs: &TempConfigStore,
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

    let object_store = match db_config.merkle_tree.mode {
        MerkleTreeMode::Lightweight => None,
        MerkleTreeMode::Full => Some(store_factory.create_store().await),
    };

    run_tree(
        task_futures,
        app_health,
        &postgres_config,
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
    postgres_config: &PostgresConfig,
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
    let metadata_calculator = MetadataCalculator::new(config, object_store)
        .await
        .context("failed initializing metadata_calculator")?;
    if let Some(api_config) = api_config {
        let address = (Ipv4Addr::UNSPECIFIED, api_config.port).into();
        let tree_reader = metadata_calculator.tree_reader();
        let stop_receiver = stop_receiver.clone();
        task_futures.push(tokio::spawn(async move {
            tree_reader
                .wait()
                .await
                .run_api_server(address, stop_receiver)
                .await
        }));
    }

    let tree_health_check = metadata_calculator.tree_health_check();
    app_health.insert_component(tree_health_check);
    let pool = ConnectionPool::<Core>::singleton(postgres_config.master_url()?)
        .build()
        .await
        .context("failed to build connection pool")?;
    let tree_task = tokio::spawn(metadata_calculator.run(pool, stop_receiver));
    task_futures.push(tree_task);

    let elapsed = started_at.elapsed();
    APP_METRICS.init_latency[&InitStage::Tree].set(elapsed);
    tracing::info!("Initialized {mode_str} tree in {elapsed:?}");
    Ok(())
}

async fn add_basic_witness_input_producer_to_task_futures(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    connection_pool: &ConnectionPool<Core>,
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

async fn add_house_keeper_to_task_futures(
    configs: &TempConfigStore,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
) -> anyhow::Result<()> {
    let house_keeper_config = configs
        .house_keeper_config
        .clone()
        .context("house_keeper_config")?;
    let postgres_config = configs.postgres_config.clone().context("postgres_config")?;
    let connection_pool = ConnectionPool::<Core>::builder(
        postgres_config.replica_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;

    let pool_for_metrics = connection_pool.clone();
    task_futures.push(tokio::spawn(async move {
        PostgresMetrics::run_scraping(pool_for_metrics, Duration::from_secs(60)).await;
        Ok(())
    }));

    let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
        house_keeper_config.l1_batch_metrics_reporting_interval_ms,
        connection_pool.clone(),
    );

    let prover_connection_pool = ConnectionPool::<Prover>::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a prover_connection_pool")?;
    task_futures.push(tokio::spawn(l1_batch_metrics_reporter.run()));

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

    let fri_prover_group_config = configs
        .fri_prover_group_config
        .clone()
        .context("fri_prover_group_config")?;
    let fri_prover_stats_reporter = FriProverStatsReporter::new(
        house_keeper_config.fri_prover_stats_reporting_interval_ms,
        prover_connection_pool.clone(),
        connection_pool.clone(),
        fri_prover_group_config,
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
    replica_connection_pool: &ConnectionPool<Core>,
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    stop_receiver: watch::Receiver<bool>,
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
        let values_cache_task = storage_caches
            .configure_storage_values_cache(values_capacity, replica_connection_pool.clone());
        task_futures.push(tokio::task::spawn(values_cache_task.run(stop_receiver)));
    }
    Ok(storage_caches)
}

async fn build_tx_sender(
    tx_sender_config: &TxSenderConfig,
    web3_json_config: &Web3JsonRpcConfig,
    state_keeper_config: &StateKeeperConfig,
    replica_pool: ConnectionPool<Core>,
    master_pool: ConnectionPool<Core>,
    batch_fee_model_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    storage_caches: PostgresStorageCaches,
) -> (TxSender, VmConcurrencyBarrier) {
    let sequencer_sealer = SequencerSealer::new(state_keeper_config.clone());
    let master_pool_sink = MasterPoolSink::new(master_pool);
    let tx_sender_builder = TxSenderBuilder::new(
        tx_sender_config.clone(),
        replica_pool.clone(),
        Arc::new(master_pool_sink),
    )
    .with_sealer(Arc::new(sequencer_sealer));

    let max_concurrency = web3_json_config.vm_concurrency_limit();
    let (vm_concurrency_limiter, vm_barrier) = VmConcurrencyLimiter::new(max_concurrency);

    let batch_fee_input_provider =
        ApiFeeInputProvider::new(batch_fee_model_input_provider, replica_pool);

    let tx_sender = tx_sender_builder
        .build(
            Arc::new(batch_fee_input_provider),
            Arc::new(vm_concurrency_limiter),
            ApiContracts::load_from_disk(),
            storage_caches,
        )
        .await;
    (tx_sender, vm_barrier)
}

#[allow(clippy::too_many_arguments)]
async fn run_http_api(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    postgres_config: &PostgresConfig,
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
    .await;

    let mut namespaces = Namespace::DEFAULT.to_vec();
    if with_debug_namespace {
        namespaces.push(Namespace::Debug)
    }
    namespaces.push(Namespace::Snapshots);

    let updaters_pool = ConnectionPool::<Core>::builder(postgres_config.replica_url()?, 2)
        .build()
        .await
        .context("failed to build last_miniblock_pool")?;

    let mut api_builder =
        web3::ApiBuilder::jsonrpsee_backend(internal_api.clone(), replica_connection_pool)
            .http(api_config.web3_json_rpc.http_port)
            .with_updaters_pool(updaters_pool)
            .with_filter_limit(api_config.web3_json_rpc.filters_limit())
            .with_batch_request_size_limit(api_config.web3_json_rpc.max_batch_request_size())
            .with_response_body_size_limit(api_config.web3_json_rpc.max_response_body_size())
            .with_tx_sender(tx_sender)
            .with_vm_barrier(vm_barrier)
            .enable_api_namespaces(namespaces);
    if let Some(tree_api_url) = api_config.web3_json_rpc.tree_api_url() {
        let tree_api = Arc::new(TreeApiHttpClient::new(tree_api_url));
        api_builder = api_builder.with_tree_api(tree_api.clone());
        app_health.insert_custom_component(tree_api);
    }

    let server_handles = api_builder
        .build()
        .context("failed to build HTTP API server")?
        .run(stop_receiver)
        .await?;
    task_futures.extend(server_handles.tasks);
    app_health.insert_component(server_handles.health_check);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_ws_api(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    postgres_config: &PostgresConfig,
    tx_sender_config: &TxSenderConfig,
    state_keeper_config: &StateKeeperConfig,
    internal_api: &InternalApiConfig,
    api_config: &ApiConfig,
    batch_fee_model_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    master_connection_pool: ConnectionPool<Core>,
    replica_connection_pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
    storage_caches: PostgresStorageCaches,
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
    .await;
    let last_miniblock_pool = ConnectionPool::<Core>::singleton(postgres_config.replica_url()?)
        .build()
        .await
        .context("failed to build last_miniblock_pool")?;

    let mut namespaces = Namespace::DEFAULT.to_vec();
    namespaces.push(Namespace::Snapshots);

    let mut api_builder =
        web3::ApiBuilder::jsonrpsee_backend(internal_api.clone(), replica_connection_pool)
            .ws(api_config.web3_json_rpc.ws_port)
            .with_updaters_pool(last_miniblock_pool)
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
            .enable_api_namespaces(namespaces);
    if let Some(tree_api_url) = api_config.web3_json_rpc.tree_api_url() {
        let tree_api = Arc::new(TreeApiHttpClient::new(tree_api_url));
        api_builder = api_builder.with_tree_api(tree_api.clone());
        app_health.insert_custom_component(tree_api);
    }

    let server_handles = api_builder
        .build()
        .context("failed to build WS API server")?
        .run(stop_receiver)
        .await?;
    task_futures.extend(server_handles.tasks);
    app_health.insert_component(server_handles.health_check);
    Ok(())
}

async fn circuit_breakers_for_components(
    components: &[Component],
    postgres_config: &PostgresConfig,
    circuit_breaker_config: &CircuitBreakerConfig,
) -> anyhow::Result<Vec<Box<dyn CircuitBreaker>>> {
    let mut circuit_breakers: Vec<Box<dyn CircuitBreaker>> = Vec::new();

    if components
        .iter()
        .any(|c| matches!(c, Component::EthTxAggregator | Component::EthTxManager))
    {
        let pool = ConnectionPool::<Core>::singleton(postgres_config.replica_url()?)
            .build()
            .await
            .context("failed to build a connection pool")?;
        circuit_breakers.push(Box::new(FailedL1TransactionChecker { pool }));
    }

    if components.iter().any(|c| {
        matches!(
            c,
            Component::HttpApi | Component::WsApi | Component::ContractVerificationApi
        )
    }) {
        let pool = ConnectionPool::<Core>::singleton(postgres_config.replica_url()?)
            .build()
            .await?;
        circuit_breakers.push(Box::new(ReplicationLagChecker {
            pool,
            replication_lag_limit_sec: circuit_breaker_config.replication_lag_limit_sec,
        }));
    }
    Ok(circuit_breakers)
}
