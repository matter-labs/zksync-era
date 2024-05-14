use std::{collections::HashSet, net::Ipv4Addr, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use metrics::EN_METRICS;
use tokio::{
    sync::{oneshot, watch, RwLock},
    task::{self, JoinHandle},
};
use zksync_block_reverter::{BlockReverter, NodeRole};
use zksync_commitment_generator::{
    commitment_post_processor::{
        CommitmentPostProcessor, RollupCommitmentPostProcessor, ValidiumCommitmentPostProcessor,
    },
    input_generation::{InputGenerator, RollupInputGenerator, ValidiumInputGenerator},
    CommitmentGenerator,
};
use zksync_concurrency::{ctx, scope};
use zksync_config::configs::{
    api::MerkleTreeApiConfig, chain::L1BatchCommitDataGeneratorMode, database::MerkleTreeMode,
};
use zksync_core::{
    api_server::{
        execution_sandbox::VmConcurrencyLimiter,
        healthcheck::HealthCheckHandle,
        tree::{TreeApiClient, TreeApiHttpClient},
        tx_sender::{proxy::TxProxy, ApiContracts, TxSenderBuilder},
        web3::{mempool_cache::MempoolCache, ApiBuilder, Namespace},
    },
    consensus,
    consistency_checker::ConsistencyChecker,
    metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig},
    reorg_detector::{self, ReorgDetector},
    setup_sigint_handler,
    state_keeper::{
        seal_criteria::NoopSealer, AsyncRocksdbCache, BatchExecutor, MainBatchExecutor,
        OutputHandler, StateKeeperPersistence, ZkSyncStateKeeper,
    },
    sync_layer::{
        batch_status_updater::BatchStatusUpdater, external_io::ExternalIO, ActionQueue, SyncState,
    },
    utils::ensure_l1_batch_commit_data_generation_mode,
};
use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core, CoreDal};
use zksync_db_connection::{
    connection_pool::ConnectionPoolBuilder, healthcheck::ConnectionPoolHealthCheck,
};
use zksync_eth_client::{clients::QueryClient, EthInterface};
use zksync_eth_sender::l1_batch_commit_data_generator::{
    L1BatchCommitDataGenerator, RollupModeL1BatchCommitDataGenerator,
    ValidiumModeL1BatchCommitDataGenerator,
};
use zksync_health_check::{AppHealthCheck, HealthStatus, ReactiveHealthCheck};
use zksync_node_db_pruner::{DbPruner, DbPrunerConfig};
use zksync_node_fee_model::l1_gas_price::MainNodeFeeParamsFetcher;
use zksync_state::{PostgresStorageCaches, RocksdbStorageOptions};
use zksync_storage::RocksDB;
use zksync_types::L2ChainId;
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_web3_decl::{
    client::{BoxedL2Client, L2Client},
    jsonrpsee,
    namespaces::EnNamespaceClient,
};

use crate::{
    config::{
        observability::ObservabilityENConfig, ExternalNodeConfig, OptionalENConfig,
        RequiredENConfig,
    },
    helpers::{EthClientHealthCheck, MainNodeHealthCheck, ValidateChainIdsTask},
    init::ensure_storage_initialized,
    metrics::RUST_METRICS,
};

mod config;
mod helpers;
mod init;
mod metadata;
mod metrics;
#[cfg(test)]
mod tests;
mod version_sync_task;

/// Creates the state keeper configured to work in the external node mode.
#[allow(clippy::too_many_arguments)]
async fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool<Core>,
    main_node_client: BoxedL2Client,
    output_handler: OutputHandler,
    stop_receiver: watch::Receiver<bool>,
    chain_id: L2ChainId,
    task_handles: &mut Vec<JoinHandle<anyhow::Result<()>>>,
) -> anyhow::Result<ZkSyncStateKeeper> {
    // We only need call traces on the external node if the `debug_` namespace is enabled.
    let save_call_traces = config.optional.api_namespaces().contains(&Namespace::Debug);

    let cache_options = RocksdbStorageOptions {
        block_cache_capacity: config.experimental.state_keeper_db_block_cache_capacity(),
        max_open_files: config.experimental.state_keeper_db_max_open_files,
    };
    let (storage_factory, task) =
        AsyncRocksdbCache::new(connection_pool.clone(), state_keeper_db_path, cache_options);
    let mut stop_receiver_clone = stop_receiver.clone();
    task_handles.push(tokio::spawn(async move {
        let result = task.run(stop_receiver_clone.clone()).await;
        stop_receiver_clone.changed().await?;
        result
    }));
    let batch_executor_base: Box<dyn BatchExecutor> = Box::new(MainBatchExecutor::new(
        Arc::new(storage_factory),
        save_call_traces,
        true,
    ));

    let io = ExternalIO::new(
        connection_pool,
        action_queue,
        Box::new(main_node_client.for_component("external_io")),
        chain_id,
    )
    .await
    .context("Failed initializing I/O for external node state keeper")?;

    Ok(ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        batch_executor_base,
        output_handler,
        Arc::new(NoopSealer),
    ))
}

async fn run_tree(
    task_futures: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    config: &ExternalNodeConfig,
    api_config: Option<&MerkleTreeApiConfig>,
    app_health: &AppHealthCheck,
    stop_receiver: watch::Receiver<bool>,
    tree_pool: ConnectionPool<Core>,
) -> anyhow::Result<Arc<dyn TreeApiClient>> {
    let metadata_calculator_config = MetadataCalculatorConfig {
        db_path: config.required.merkle_tree_path.clone(),
        max_open_files: config.optional.merkle_tree_max_open_files,
        mode: MerkleTreeMode::Lightweight,
        delay_interval: config.optional.metadata_calculator_delay(),
        max_l1_batches_per_iter: config.optional.max_l1_batches_per_tree_iter,
        multi_get_chunk_size: config.optional.merkle_tree_multi_get_chunk_size,
        block_cache_capacity: config.optional.merkle_tree_block_cache_size(),
        include_indices_and_filters_in_block_cache: config
            .optional
            .merkle_tree_include_indices_and_filters_in_block_cache,
        memtable_capacity: config.optional.merkle_tree_memtable_capacity(),
        stalled_writes_timeout: config.optional.merkle_tree_stalled_writes_timeout(),
    };

    let max_concurrency = config
        .optional
        .snapshots_recovery_postgres_max_concurrency
        .get();
    let max_concurrency = u32::try_from(max_concurrency).with_context(|| {
        format!("snapshot recovery max concurrency ({max_concurrency}) is too large")
    })?;
    let recovery_pool = ConnectionPool::builder(
        tree_pool.database_url().clone(),
        max_concurrency.min(config.postgres.max_connections),
    )
    .build()
    .await
    .context("failed creating DB pool for Merkle tree recovery")?;

    let mut metadata_calculator =
        MetadataCalculator::new(metadata_calculator_config, None, tree_pool)
            .await
            .context("failed initializing metadata calculator")?
            .with_recovery_pool(recovery_pool);

    let tree_reader = Arc::new(metadata_calculator.tree_reader());
    app_health.insert_custom_component(Arc::new(metadata_calculator.tree_health_check()))?;

    if config.optional.pruning_enabled {
        tracing::warn!("Proceeding with node state pruning for the Merkle tree. This is an experimental feature; use at your own risk");

        let pruning_task =
            metadata_calculator.pruning_task(config.optional.pruning_removal_delay() / 2);
        app_health.insert_component(pruning_task.health_check())?;
        let pruning_task_handle = tokio::spawn(pruning_task.run(stop_receiver.clone()));
        task_futures.push(pruning_task_handle);
    }

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

    let tree_handle = task::spawn(metadata_calculator.run(stop_receiver));

    task_futures.push(tree_handle);
    Ok(tree_reader)
}

#[allow(clippy::too_many_arguments)]
async fn run_core(
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool<Core>,
    main_node_client: BoxedL2Client,
    eth_client: Box<dyn EthInterface>,
    task_handles: &mut Vec<task::JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    stop_receiver: watch::Receiver<bool>,
    fee_params_fetcher: Arc<MainNodeFeeParamsFetcher>,
    singleton_pool_builder: &ConnectionPoolBuilder<Core>,
) -> anyhow::Result<SyncState> {
    // Create components.
    let sync_state = SyncState::default();
    app_health.insert_custom_component(Arc::new(sync_state.clone()))?;
    let (action_queue_sender, action_queue) = ActionQueue::new();

    let (persistence, miniblock_sealer) = StateKeeperPersistence::new(
        connection_pool.clone(),
        config
            .remote
            .l2_shared_bridge_addr
            .expect("L2 shared bridge address is not set"),
        config.optional.miniblock_seal_queue_capacity,
    );
    task_handles.push(tokio::spawn(miniblock_sealer.run()));

    let mut persistence = persistence.with_tx_insertion();
    if !config.optional.protective_reads_persistence_enabled {
        // **Important:** Disabling protective reads persistence is only sound if the node will never
        // run a full Merkle tree.
        tracing::warn!("Disabling persisting protective reads; this should be safe, but is considered an experimental option at the moment");
        persistence = persistence.without_protective_reads();
    }

    let output_handler =
        OutputHandler::new(Box::new(persistence)).with_handler(Box::new(sync_state.clone()));
    let state_keeper = build_state_keeper(
        action_queue,
        config.required.state_cache_path.clone(),
        config,
        connection_pool.clone(),
        main_node_client.clone(),
        output_handler,
        stop_receiver.clone(),
        config.required.l2_chain_id,
        task_handles,
    )
    .await?;

    task_handles.push(tokio::spawn({
        let config = config.consensus.clone();
        let secrets =
            config::read_consensus_secrets().context("config::read_consensus_secrets()")?;
        let cfg = match (config, secrets) {
            (Some(cfg), Some(secrets)) => Some((cfg, secrets)),
            (Some(_), None) => {
                anyhow::bail!("Consensus config is specified, but secrets are missing")
            }
            (None, _) => {
                // Secrets may be unconditionally embedded in some environments, but they are unused
                // unless a consensus config is provided.
                None
            }
        };

        let pool = connection_pool.clone();
        let sync_state = sync_state.clone();
        let main_node_client = main_node_client.clone();
        let mut stop_receiver = stop_receiver.clone();
        async move {
            // We instantiate the root context here, since the consensus task is the only user of the
            // structured concurrency framework.
            // Note, however, that awaiting for the `stop_receiver` is related to the root context behavior,
            // not the consensus task itself. There may have been any number of tasks running in the root context,
            // but we only need to wait for stop signal once, and it will be propagated to all child contexts.
            let ctx = ctx::root();
            scope::run!(&ctx, |ctx, s| async move {
                s.spawn_bg(consensus::era::run_fetcher(
                    ctx,
                    cfg,
                    pool,
                    sync_state,
                    main_node_client,
                    action_queue_sender,
                ));
                ctx.wait(stop_receiver.wait_for(|stop| *stop)).await??;
                Ok(())
            })
            .await
            .context("consensus actor")
        }
    }));

    if config.optional.pruning_enabled {
        tracing::warn!("Proceeding with node state pruning for Postgres. This is an experimental feature; use at your own risk");

        let minimum_l1_batch_age = config.optional.pruning_data_retention();
        tracing::info!(
            "Configured pruning of batches after they become {minimum_l1_batch_age:?} old"
        );
        let db_pruner = DbPruner::new(
            DbPrunerConfig {
                removal_delay: config.optional.pruning_removal_delay(),
                pruned_batch_chunk_size: config.optional.pruning_chunk_size,
                minimum_l1_batch_age,
            },
            connection_pool.clone(),
        );
        app_health.insert_component(db_pruner.health_check())?;
        task_handles.push(tokio::spawn(db_pruner.run(stop_receiver.clone())));
    }

    let sk_handle = task::spawn(state_keeper.run());
    let fee_params_fetcher_handle =
        tokio::spawn(fee_params_fetcher.clone().run(stop_receiver.clone()));
    let remote_diamond_proxy_addr = config.remote.diamond_proxy_addr;
    let diamond_proxy_addr = if let Some(addr) = config.optional.contracts_diamond_proxy_addr {
        anyhow::ensure!(
            addr == remote_diamond_proxy_addr,
            "Diamond proxy address {addr:?} specified in config doesn't match one returned \
            by main node ({remote_diamond_proxy_addr:?})"
        );
        addr
    } else {
        tracing::info!(
            "Diamond proxy address is not specified in config; will use address \
            returned by main node: {remote_diamond_proxy_addr:?}"
        );
        remote_diamond_proxy_addr
    };

    ensure_l1_batch_commit_data_generation_mode(
        config.optional.l1_batch_commit_data_generator_mode,
        diamond_proxy_addr,
        eth_client.as_ref(),
    )
    .await?;

    let (l1_batch_commit_data_generator, input_generator, commitment_post_processor): (
        Arc<dyn L1BatchCommitDataGenerator>,
        Box<dyn InputGenerator>,
        Box<dyn CommitmentPostProcessor>,
    ) = match config.optional.l1_batch_commit_data_generator_mode {
        L1BatchCommitDataGeneratorMode::Rollup => (
            Arc::new(RollupModeL1BatchCommitDataGenerator {}),
            Box::new(RollupInputGenerator),
            Box::new(RollupCommitmentPostProcessor),
        ),
        L1BatchCommitDataGeneratorMode::Validium => (
            Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
            Box::new(ValidiumInputGenerator),
            Box::new(ValidiumCommitmentPostProcessor),
        ),
    };

    let consistency_checker = ConsistencyChecker::new(
        eth_client,
        10, // TODO (BFT-97): Make it a part of a proper EN config
        singleton_pool_builder
            .build()
            .await
            .context("failed to build connection pool for ConsistencyChecker")?,
        l1_batch_commit_data_generator,
    )
    .context("cannot initialize consistency checker")?
    .with_diamond_proxy_addr(diamond_proxy_addr);

    app_health.insert_component(consistency_checker.health_check().clone())?;
    let consistency_checker_handle = tokio::spawn(consistency_checker.run(stop_receiver.clone()));

    let batch_status_updater = BatchStatusUpdater::new(
        main_node_client.clone(),
        singleton_pool_builder
            .build()
            .await
            .context("failed to build a connection pool for BatchStatusUpdater")?,
    );
    app_health.insert_component(batch_status_updater.health_check())?;

    let commitment_generator_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a commitment_generator_pool")?;
    let commitment_generator = CommitmentGenerator::new(
        commitment_generator_pool,
        input_generator,
        commitment_post_processor,
    );
    app_health.insert_component(commitment_generator.health_check())?;
    let commitment_generator_handle = tokio::spawn(commitment_generator.run(stop_receiver.clone()));

    let updater_handle = task::spawn(batch_status_updater.run(stop_receiver.clone()));

    task_handles.extend([
        sk_handle,
        fee_params_fetcher_handle,
        consistency_checker_handle,
        commitment_generator_handle,
        updater_handle,
    ]);

    Ok(sync_state)
}

#[allow(clippy::too_many_arguments)]
async fn run_api(
    task_handles: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    config: &ExternalNodeConfig,
    app_health: &AppHealthCheck,
    connection_pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
    sync_state: SyncState,
    tree_reader: Option<Arc<dyn TreeApiClient>>,
    main_node_client: BoxedL2Client,
    singleton_pool_builder: &ConnectionPoolBuilder<Core>,
    fee_params_fetcher: Arc<MainNodeFeeParamsFetcher>,
    components: &HashSet<Component>,
) -> anyhow::Result<()> {
    let tree_reader = match tree_reader {
        Some(tree_reader) => {
            if let Some(url) = &config.api_component.tree_api_remote_url {
                tracing::warn!(
                    "Tree component is run locally; the specified tree API URL {url} is ignored"
                );
            }
            Some(tree_reader)
        }
        None => config
            .api_component
            .tree_api_remote_url
            .as_ref()
            .map(|url| Arc::new(TreeApiHttpClient::new(url)) as Arc<dyn TreeApiClient>),
    };
    if tree_reader.is_none() {
        tracing::info!(
            "Tree reader is not set; `zks_getProof` RPC method will be unavailable. To enable, \
             either specify `tree_api_url` for the API component, or run the tree in the same process as API"
        );
    }

    let tx_proxy = TxProxy::new(main_node_client.clone());
    let proxy_cache_updater_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a proxy_cache_updater_pool")?;
    task_handles.push(tokio::spawn(tx_proxy.run_account_nonce_sweeper(
        proxy_cache_updater_pool.clone(),
        stop_receiver.clone(),
    )));

    let tx_sender_builder =
        TxSenderBuilder::new(config.into(), connection_pool.clone(), Arc::new(tx_proxy));

    if config.optional.transactions_per_sec_limit.is_some() {
        tracing::warn!("`transactions_per_sec_limit` option is deprecated and ignored");
    };

    let max_concurrency = config.optional.vm_concurrency_limit;
    let (vm_concurrency_limiter, vm_barrier) = VmConcurrencyLimiter::new(max_concurrency);
    let mut storage_caches = PostgresStorageCaches::new(
        config.optional.factory_deps_cache_size() as u64,
        config.optional.initial_writes_cache_size() as u64,
    );
    let latest_values_cache_size = config.optional.latest_values_cache_size() as u64;
    let cache_update_handle = (latest_values_cache_size > 0).then(|| {
        task::spawn(
            storage_caches
                .configure_storage_values_cache(latest_values_cache_size, connection_pool.clone())
                .run(stop_receiver.clone()),
        )
    });
    task_handles.extend(cache_update_handle);

    let whitelisted_tokens_for_aa_cache = Arc::new(RwLock::new(Vec::new()));
    let whitelisted_tokens_for_aa_cache_clone = whitelisted_tokens_for_aa_cache.clone();
    let mut stop_receiver_for_task = stop_receiver.clone();
    task_handles.push(task::spawn(async move {
        while !*stop_receiver_for_task.borrow_and_update() {
            match main_node_client.whitelisted_tokens_for_aa().await {
                Ok(tokens) => {
                    *whitelisted_tokens_for_aa_cache_clone.write().await = tokens;
                }
                Err(jsonrpsee::core::client::Error::Call(error))
                    if error.code() == jsonrpsee::types::error::METHOD_NOT_FOUND_CODE =>
                {
                    // Method is not supported by the main node, do nothing.
                }
                Err(err) => {
                    tracing::error!("Failed to query `whitelisted_tokens_for_aa`, error: {err:?}");
                }
            }

            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(Duration::from_secs(60), stop_receiver_for_task.changed())
                .await
                .ok();
        }
        Ok(())
    }));

    let tx_sender = tx_sender_builder
        .with_whitelisted_tokens_for_aa(whitelisted_tokens_for_aa_cache)
        .build(
            fee_params_fetcher,
            Arc::new(vm_concurrency_limiter),
            ApiContracts::load_from_disk(), // TODO (BFT-138): Allow to dynamically reload API contracts
            storage_caches,
        )
        .await;

    let mempool_cache = MempoolCache::new(config.optional.mempool_cache_size);
    let mempool_cache_update_task = mempool_cache.update_task(
        connection_pool.clone(),
        config.optional.mempool_cache_update_interval(),
    );
    task_handles.push(tokio::spawn(
        mempool_cache_update_task.run(stop_receiver.clone()),
    ));

    // The refresh interval should be several times lower than the pruning removal delay, so that
    // soft-pruning will timely propagate to the API server.
    let pruning_info_refresh_interval = config.optional.pruning_removal_delay() / 5;

    if components.contains(&Component::HttpApi) {
        let mut builder = ApiBuilder::jsonrpsee_backend(config.into(), connection_pool.clone())
            .http(config.required.http_port)
            .with_filter_limit(config.optional.filters_limit)
            .with_batch_request_size_limit(config.optional.max_batch_request_size)
            .with_response_body_size_limit(config.optional.max_response_body_size())
            .with_pruning_info_refresh_interval(pruning_info_refresh_interval)
            .with_tx_sender(tx_sender.clone())
            .with_vm_barrier(vm_barrier.clone())
            .with_sync_state(sync_state.clone())
            .with_mempool_cache(mempool_cache.clone())
            .with_extended_tracing(config.optional.extended_rpc_tracing)
            .enable_api_namespaces(config.optional.api_namespaces());
        if let Some(tree_reader) = &tree_reader {
            builder = builder.with_tree_api(tree_reader.clone());
        }

        let http_server_handles = builder
            .build()
            .context("failed to build HTTP JSON-RPC server")?
            .run(stop_receiver.clone())
            .await
            .context("Failed initializing HTTP JSON-RPC server")?;
        app_health.insert_component(http_server_handles.health_check)?;
        task_handles.extend(http_server_handles.tasks);
    }

    if components.contains(&Component::WsApi) {
        let mut builder = ApiBuilder::jsonrpsee_backend(config.into(), connection_pool.clone())
            .ws(config.required.ws_port)
            .with_filter_limit(config.optional.filters_limit)
            .with_subscriptions_limit(config.optional.subscriptions_limit)
            .with_batch_request_size_limit(config.optional.max_batch_request_size)
            .with_response_body_size_limit(config.optional.max_response_body_size())
            .with_polling_interval(config.optional.polling_interval())
            .with_pruning_info_refresh_interval(pruning_info_refresh_interval)
            .with_tx_sender(tx_sender)
            .with_vm_barrier(vm_barrier)
            .with_sync_state(sync_state)
            .with_mempool_cache(mempool_cache)
            .with_extended_tracing(config.optional.extended_rpc_tracing)
            .enable_api_namespaces(config.optional.api_namespaces());
        if let Some(tree_reader) = tree_reader {
            builder = builder.with_tree_api(tree_reader);
        }

        let ws_server_handles = builder
            .build()
            .context("failed to build WS JSON-RPC server")?
            .run(stop_receiver.clone())
            .await
            .context("Failed initializing WS JSON-RPC server")?;
        app_health.insert_component(ws_server_handles.health_check)?;
        task_handles.extend(ws_server_handles.tasks);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn init_tasks(
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool<Core>,
    singleton_pool_builder: ConnectionPoolBuilder<Core>,
    main_node_client: BoxedL2Client,
    eth_client: Box<dyn EthInterface>,
    task_handles: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    stop_receiver: watch::Receiver<bool>,
    components: &HashSet<Component>,
) -> anyhow::Result<()> {
    let protocol_version_update_task =
        EN_METRICS.run_protocol_version_updates(connection_pool.clone(), stop_receiver.clone());
    task_handles.push(tokio::spawn(protocol_version_update_task));

    // Run the components.
    let tree_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a tree_pool")?;

    if !components.contains(&Component::Tree) {
        anyhow::ensure!(
            !components.contains(&Component::TreeApi),
            "Merkle tree API cannot be started without a tree component"
        );
    }
    // Create a tree reader. If the list of requested components has the tree itself, then
    // we can get this tree's reader and use it right away. Otherwise, if configuration has
    // specified address of another instance hosting tree API, create a tree reader to that
    // remote API. A tree reader is necessary for `zks_getProof` method to work.
    let tree_reader: Option<Arc<dyn TreeApiClient>> = if components.contains(&Component::Tree) {
        let tree_api_config = if components.contains(&Component::TreeApi) {
            Some(MerkleTreeApiConfig {
                port: config
                    .tree_component
                    .api_port
                    .context("should contain tree api port")?,
            })
        } else {
            None
        };
        Some(
            run_tree(
                task_handles,
                config,
                tree_api_config.as_ref(),
                app_health,
                stop_receiver.clone(),
                tree_pool,
            )
            .await?,
        )
    } else {
        None
    };

    let fee_params_fetcher = Arc::new(MainNodeFeeParamsFetcher::new(main_node_client.clone()));

    let sync_state = if components.contains(&Component::Core) {
        run_core(
            config,
            connection_pool.clone(),
            main_node_client.clone(),
            eth_client,
            task_handles,
            app_health,
            stop_receiver.clone(),
            fee_params_fetcher.clone(),
            &singleton_pool_builder,
        )
        .await?
    } else {
        let sync_state = SyncState::default();

        task_handles.push(tokio::spawn(sync_state.clone().run_updater(
            connection_pool.clone(),
            main_node_client.clone(),
            stop_receiver.clone(),
        )));

        sync_state
    };

    if components.contains(&Component::HttpApi) || components.contains(&Component::WsApi) {
        run_api(
            task_handles,
            config,
            app_health,
            connection_pool,
            stop_receiver.clone(),
            sync_state,
            tree_reader,
            main_node_client,
            &singleton_pool_builder,
            fee_params_fetcher.clone(),
            components,
        )
        .await?;
    }

    if let Some(prometheus) = config.observability.prometheus() {
        tracing::info!("Starting Prometheus exporter with configuration: {prometheus:?}");

        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");
        app_health.insert_component(prometheus_health_check)?;
        task_handles.push(tokio::spawn(async move {
            prometheus_health_updater.update(HealthStatus::Ready.into());
            let result = prometheus.run(stop_receiver).await;
            drop(prometheus_health_updater);
            result
        }));
    }

    Ok(())
}

async fn shutdown_components(
    stop_sender: watch::Sender<bool>,
    tasks: ManagedTasks,
    healthcheck_handle: HealthCheckHandle,
) -> anyhow::Result<()> {
    stop_sender.send(true).ok();
    task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .context("error waiting for RocksDB instances to drop")?;
    // Increase timeout because of complicated graceful shutdown procedure for API servers.
    tasks.complete(Duration::from_secs(30)).await;
    healthcheck_handle.stop().await;
    Ok(())
}

/// External node for zkSync Era.
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Revert the pending L1 batch and exit.
    #[arg(long)]
    revert_pending_l1_batch: bool,
    /// Enables consensus-based syncing instead of JSON-RPC based one. This is an experimental and incomplete feature;
    /// do not use unless you know what you're doing.
    #[arg(long)]
    enable_consensus: bool,

    /// Comma-separated list of components to launch.
    #[arg(long, default_value = "all")]
    components: ComponentsToRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub enum Component {
    HttpApi,
    WsApi,
    Tree,
    TreeApi,
    Core,
}

impl Component {
    fn components_from_str(s: &str) -> anyhow::Result<&[Component]> {
        match s {
            "api" => Ok(&[Component::HttpApi, Component::WsApi]),
            "http_api" => Ok(&[Component::HttpApi]),
            "ws_api" => Ok(&[Component::WsApi]),
            "tree" => Ok(&[Component::Tree]),
            "tree_api" => Ok(&[Component::TreeApi]),
            "core" => Ok(&[Component::Core]),
            "all" => Ok(&[
                Component::HttpApi,
                Component::WsApi,
                Component::Tree,
                Component::Core,
            ]),
            other => Err(anyhow::anyhow!("{other} is not a valid component name")),
        }
    }
}

#[derive(Debug, Clone)]
struct ComponentsToRun(HashSet<Component>);

impl FromStr for ComponentsToRun {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let components = s
            .split(',')
            .try_fold(HashSet::new(), |mut acc, component_str| {
                let components = Component::components_from_str(component_str.trim())?;
                acc.extend(components);
                Ok::<_, Self::Err>(acc)
            })?;
        Ok(Self(components))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial setup.
    let opt = Cli::parse();

    let observability_config =
        ObservabilityENConfig::from_env().context("ObservabilityENConfig::from_env()")?;
    let _guard = observability_config.build_observability()?;
    let required_config = RequiredENConfig::from_env()?;
    let optional_config = OptionalENConfig::from_env()?;

    // Build L1 and L2 clients.
    let main_node_url = &required_config.main_node_url;
    tracing::info!("Main node URL is: {main_node_url:?}");
    let main_node_client = L2Client::http(main_node_url.clone())
        .context("Failed creating JSON-RPC client for main node")?
        .with_allowed_requests_per_second(optional_config.main_node_rate_limit_rps)
        .build();
    let main_node_client = BoxedL2Client::new(main_node_client);

    let eth_client_url = &required_config.eth_client_url;
    let eth_client = Box::new(QueryClient::new(eth_client_url.clone())?);

    let mut config = ExternalNodeConfig::new(
        required_config,
        optional_config,
        observability_config,
        &main_node_client,
    )
    .await
    .context("Failed to load external node config")?;
    if !opt.enable_consensus {
        config.consensus = None;
    }
    if let Some(threshold) = config.optional.slow_query_threshold() {
        ConnectionPool::<Core>::global_config().set_slow_query_threshold(threshold)?;
    }
    if let Some(threshold) = config.optional.long_connection_threshold() {
        ConnectionPool::<Core>::global_config().set_long_connection_threshold(threshold)?;
    }

    RUST_METRICS.initialize();
    EN_METRICS.observe_config(&config);

    let singleton_pool_builder = ConnectionPool::singleton(config.postgres.database_url());
    let connection_pool = ConnectionPool::<Core>::builder(
        config.postgres.database_url(),
        config.postgres.max_connections,
    )
    .build()
    .await
    .context("failed to build a connection_pool")?;

    run_node(
        (),
        &opt,
        &config,
        connection_pool,
        singleton_pool_builder,
        main_node_client,
        eth_client,
    )
    .await
}

/// Environment for the node encapsulating its interactions. Used in EN tests to mock signal sending etc.
trait NodeEnvironment {
    /// Sets the SIGINT handler, returning a future that will resolve when a signal is sent.
    fn setup_sigint_handler(&mut self) -> oneshot::Receiver<()>;

    /// Sets the application health of the node.
    fn set_app_health(&mut self, health: Arc<AppHealthCheck>);
}

impl NodeEnvironment for () {
    fn setup_sigint_handler(&mut self) -> oneshot::Receiver<()> {
        setup_sigint_handler()
    }

    fn set_app_health(&mut self, _health: Arc<AppHealthCheck>) {
        // Do nothing
    }
}

async fn run_node(
    mut env: impl NodeEnvironment,
    opt: &Cli,
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool<Core>,
    singleton_pool_builder: ConnectionPoolBuilder<Core>,
    main_node_client: BoxedL2Client,
    eth_client: Box<dyn EthInterface>,
) -> anyhow::Result<()> {
    tracing::warn!("The external node is in the alpha phase, and should be used with caution.");
    tracing::info!("Started the external node");
    let (stop_sender, stop_receiver) = watch::channel(false);

    let app_health = Arc::new(AppHealthCheck::new(
        config.optional.healthcheck_slow_time_limit(),
        config.optional.healthcheck_hard_time_limit(),
    ));
    app_health.insert_custom_component(Arc::new(MainNodeHealthCheck::from(
        main_node_client.clone(),
    )))?;
    app_health.insert_custom_component(Arc::new(EthClientHealthCheck::from(eth_client.clone())))?;
    app_health.insert_custom_component(Arc::new(ConnectionPoolHealthCheck::new(
        connection_pool.clone(),
    )))?;

    // Start the health check server early into the node lifecycle so that its health can be monitored from the very start.
    let healthcheck_handle = HealthCheckHandle::spawn_server(
        ([0, 0, 0, 0], config.required.healthcheck_port).into(),
        app_health.clone(),
    );
    // Start scraping Postgres metrics before store initialization as well.
    let pool_for_metrics = singleton_pool_builder.build().await?;
    let mut stop_receiver_for_metrics = stop_receiver.clone();
    let metrics_task = tokio::spawn(async move {
        tokio::select! {
            () = PostgresMetrics::run_scraping(pool_for_metrics, Duration::from_secs(60)) => {
                tracing::warn!("Postgres metrics scraping unexpectedly stopped");
            }
            _ = stop_receiver_for_metrics.changed() => {
                tracing::info!("Stop signal received, Postgres metrics scraping is shutting down");
            }
        }
        Ok(())
    });

    let validate_chain_ids_task = ValidateChainIdsTask::new(
        config.required.l1_chain_id,
        config.required.l2_chain_id,
        eth_client.clone(),
        main_node_client.clone(),
    );
    let validate_chain_ids_task = tokio::spawn(validate_chain_ids_task.run(stop_receiver.clone()));

    let version_sync_task_pool = connection_pool.clone();
    let version_sync_task_main_node_client = main_node_client.clone();
    let mut stop_receiver_for_version_sync = stop_receiver.clone();
    let version_sync_task = tokio::spawn(async move {
        version_sync_task::sync_versions(
            version_sync_task_pool,
            version_sync_task_main_node_client,
        )
        .await?;

        stop_receiver_for_version_sync.changed().await.ok();
        Ok(())
    });
    let mut task_handles = vec![metrics_task, validate_chain_ids_task, version_sync_task];

    // Make sure that the node storage is initialized either via genesis or snapshot recovery.
    ensure_storage_initialized(
        connection_pool.clone(),
        main_node_client.clone(),
        &app_health,
        config.required.l2_chain_id,
        config.optional.snapshots_recovery_enabled,
    )
    .await?;
    let sigint_receiver = env.setup_sigint_handler();

    // Revert the storage if needed.
    let mut reverter = BlockReverter::new(NodeRole::External, connection_pool.clone());
    // Reverting executed batches is more-or-less safe for external nodes.
    let reverter = reverter
        .allow_rolling_back_executed_batches()
        .enable_rolling_back_postgres()
        .enable_rolling_back_merkle_tree(config.required.merkle_tree_path.clone())
        .enable_rolling_back_state_keeper_cache(config.required.state_cache_path.clone());

    let mut reorg_detector = ReorgDetector::new(main_node_client.clone(), connection_pool.clone());
    // We're checking for the reorg in the beginning because we expect that if reorg is detected during
    // the node lifecycle, the node will exit the same way as it does with any other critical error,
    // and would restart. Then, on the 2nd launch reorg would be detected here, then processed and the node
    // will be able to operate normally afterwards.
    match reorg_detector.check_consistency().await {
        Ok(()) => {}
        Err(reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
            tracing::info!("Reverting to l1 batch number {last_correct_l1_batch}");
            reverter.roll_back(last_correct_l1_batch).await?;
            tracing::info!("Revert successfully completed");
        }
        Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
    }
    if opt.revert_pending_l1_batch {
        tracing::info!("Reverting pending L1 batch");
        let mut connection = connection_pool.connection().await?;
        let sealed_l1_batch_number = connection
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
            .context("Cannot revert pending L1 batch since there are no L1 batches in Postgres")?;
        drop(connection);

        tracing::info!("Reverting to l1 batch number {sealed_l1_batch_number}");
        reverter.roll_back(sealed_l1_batch_number).await?;
        tracing::info!("Revert successfully completed");
    }

    app_health.insert_component(reorg_detector.health_check().clone())?;
    task_handles.push(tokio::spawn({
        let stop = stop_receiver.clone();
        async move {
            reorg_detector
                .run(stop)
                .await
                .context("reorg_detector.run()")
        }
    }));

    init_tasks(
        config,
        connection_pool,
        singleton_pool_builder,
        main_node_client,
        eth_client,
        &mut task_handles,
        &app_health,
        stop_receiver.clone(),
        &opt.components.0,
    )
    .await
    .context("init_tasks")?;

    env.set_app_health(app_health);

    let mut tasks = ManagedTasks::new(task_handles);
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = sigint_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
    };

    // Reaching this point means that either some actor exited unexpectedly or we received a stop signal.
    // Broadcast the stop signal to all actors and exit.
    shutdown_components(stop_sender, tasks, healthcheck_handle).await?;
    tracing::info!("Stopped");
    Ok(())
}
