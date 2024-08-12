use std::{collections::HashSet, net::Ipv4Addr, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use metrics::EN_METRICS;
use node_builder::ExternalNodeBuilder;
use tokio::{
    sync::{oneshot, watch, RwLock},
    task::{self, JoinHandle},
};
use zksync_block_reverter::{BlockReverter, NodeRole};
use zksync_commitment_generator::{
    validation_task::L1BatchCommitmentModeValidationTask, CommitmentGenerator,
};
use zksync_concurrency::{ctx, scope};
use zksync_config::configs::{api::MerkleTreeApiConfig, database::MerkleTreeMode};
use zksync_consistency_checker::ConsistencyChecker;
use zksync_core_leftovers::setup_sigint_handler;
use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core};
use zksync_db_connection::connection_pool::ConnectionPoolBuilder;
use zksync_health_check::{AppHealthCheck, HealthStatus, ReactiveHealthCheck};
use zksync_metadata_calculator::{
    api_server::{TreeApiClient, TreeApiHttpClient},
    MetadataCalculator, MetadataCalculatorConfig, MetadataCalculatorRecoveryConfig,
};
use zksync_node_api_server::{
    execution_sandbox::VmConcurrencyLimiter,
    healthcheck::HealthCheckHandle,
    tx_sender::{proxy::TxProxy, ApiContracts, TxSenderBuilder},
    web3::{mempool_cache::MempoolCache, ApiBuilder, Namespace},
};
use zksync_node_consensus as consensus;
use zksync_node_db_pruner::{DbPruner, DbPrunerConfig};
use zksync_node_fee_model::l1_gas_price::MainNodeFeeParamsFetcher;
use zksync_node_sync::{
    batch_status_updater::BatchStatusUpdater, external_io::ExternalIO,
    tree_data_fetcher::TreeDataFetcher, validate_chain_ids_task::ValidateChainIdsTask, ActionQueue,
    MainNodeHealthCheck, SyncState,
};
use zksync_reorg_detector::ReorgDetector;
use zksync_shared_metrics::rustc::RUST_METRICS;
use zksync_state::{PostgresStorageCaches, RocksdbStorageOptions};
use zksync_state_keeper::{
    seal_criteria::NoopSealer, AsyncRocksdbCache, BatchExecutor, MainBatchExecutor, OutputHandler,
    StateKeeperPersistence, TreeWritesPersistence, ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;
use zksync_types::L2ChainId;
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_web3_decl::{
    client::{Client, DynClient, L1, L2},
    jsonrpsee,
    namespaces::EnNamespaceClient,
};

use crate::{
    config::{generate_consensus_secrets, ExternalNodeConfig},
    init::{ensure_storage_initialized, SnapshotRecoveryConfig},
};

mod config;
mod init;
mod metadata;
mod metrics;
mod node_builder;
#[cfg(test)]
mod tests;

/// Creates the state keeper configured to work in the external node mode.
#[allow(clippy::too_many_arguments)]
async fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool<Core>,
    main_node_client: Box<DynClient<L2>>,
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
    let batch_executor = MainBatchExecutor::new(save_call_traces, true);
    let batch_executor: Box<dyn BatchExecutor> = Box::new(batch_executor);

    let io = ExternalIO::new(
        connection_pool,
        action_queue,
        Box::new(main_node_client.for_component("external_io")),
        chain_id,
    )
    .context("Failed initializing I/O for external node state keeper")?;

    Ok(ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        batch_executor,
        output_handler,
        Arc::new(NoopSealer),
        Arc::new(storage_factory),
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
        delay_interval: config.optional.merkle_tree_processing_delay(),
        max_l1_batches_per_iter: config.optional.merkle_tree_max_l1_batches_per_iter,
        multi_get_chunk_size: config.optional.merkle_tree_multi_get_chunk_size,
        block_cache_capacity: config.optional.merkle_tree_block_cache_size(),
        include_indices_and_filters_in_block_cache: config
            .optional
            .merkle_tree_include_indices_and_filters_in_block_cache,
        memtable_capacity: config.optional.merkle_tree_memtable_capacity(),
        stalled_writes_timeout: config.optional.merkle_tree_stalled_writes_timeout(),
        sealed_batches_have_protective_reads: config.optional.protective_reads_persistence_enabled,
        recovery: MetadataCalculatorRecoveryConfig {
            desired_chunk_size: config.experimental.snapshots_recovery_tree_chunk_size,
            parallel_persistence_buffer: config
                .experimental
                .snapshots_recovery_tree_parallel_persistence_buffer,
        },
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
        let mut stop_receiver = stop_receiver.clone();
        task_futures.push(tokio::spawn(async move {
            if let Some(reader) = tree_reader.wait().await {
                reader.run_api_server(address, stop_receiver).await
            } else {
                // Tree is dropped before initialized, e.g. because the node is getting shut down.
                // We don't want to treat this as an error since it could mask the real shutdown cause in logs etc.
                tracing::warn!(
                    "Tree is dropped before initialized, not starting the tree API server"
                );
                stop_receiver.changed().await?;
                Ok(())
            }
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
    main_node_client: Box<DynClient<L2>>,
    eth_client: Box<DynClient<L1>>,
    task_handles: &mut Vec<JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    stop_receiver: watch::Receiver<bool>,
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
        config.optional.l2_block_seal_queue_capacity,
    );
    task_handles.push(tokio::spawn(miniblock_sealer.run()));

    let mut persistence = persistence.with_tx_insertion();
    if !config.optional.protective_reads_persistence_enabled {
        // **Important:** Disabling protective reads persistence is only sound if the node will never
        // run a full Merkle tree.
        tracing::warn!("Disabling persisting protective reads; this should be safe, but is considered an experimental option at the moment");
        persistence = persistence.without_protective_reads();
    }
    let tree_writes_persistence = TreeWritesPersistence::new(connection_pool.clone());

    let output_handler = OutputHandler::new(Box::new(persistence))
        .with_handler(Box::new(tree_writes_persistence))
        .with_handler(Box::new(sync_state.clone()));
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
                s.spawn_bg(consensus::era::run_external_node(
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

    // Run validation asynchronously: the node starting shouldn't depend on Ethereum client availability,
    // and the impact of a failed async check is reasonably low (the commitment mode is only used in consistency checker).
    let validation_task = L1BatchCommitmentModeValidationTask::new(
        diamond_proxy_addr,
        config.optional.l1_batch_commit_data_generator_mode,
        eth_client.clone(),
    );
    task_handles.push(tokio::spawn(validation_task.run(stop_receiver.clone())));

    let consistency_checker = ConsistencyChecker::new(
        eth_client,
        10, // TODO (BFT-97): Make it a part of a proper EN config
        singleton_pool_builder
            .build()
            .await
            .context("failed to build connection pool for ConsistencyChecker")?,
        config.optional.l1_batch_commit_data_generator_mode,
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

    let mut commitment_generator = CommitmentGenerator::new(
        connection_pool.clone(),
        config.optional.l1_batch_commit_data_generator_mode,
    );
    if let Some(parallelism) = config.experimental.commitment_generator_max_parallelism {
        commitment_generator.set_max_parallelism(parallelism);
    }
    app_health.insert_component(commitment_generator.health_check())?;
    let commitment_generator_handle = tokio::spawn(commitment_generator.run(stop_receiver.clone()));

    let updater_handle = task::spawn(batch_status_updater.run(stop_receiver.clone()));

    task_handles.extend([
        sk_handle,
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
    main_node_client: Box<DynClient<L2>>,
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
    task_handles.push(tokio::spawn(
        tx_proxy
            .account_nonce_sweeper_task(proxy_cache_updater_pool.clone())
            .run(stop_receiver.clone()),
    ));

    let fee_params_fetcher_handle =
        tokio::spawn(fee_params_fetcher.clone().run(stop_receiver.clone()));
    task_handles.push(fee_params_fetcher_handle);

    let tx_sender_builder =
        TxSenderBuilder::new(config.into(), connection_pool.clone(), Arc::new(tx_proxy));

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
            ApiContracts::load_from_disk().await?, // TODO (BFT-138): Allow to dynamically reload API contracts
            storage_caches,
        );

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
    main_node_client: Box<DynClient<L2>>,
    eth_client: Box<DynClient<L1>>,
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

    if components.contains(&Component::TreeFetcher) {
        tracing::warn!(
            "Running tree data fetcher (allows a node to operate w/o a Merkle tree or w/o waiting the tree to catch up). \
             This is an experimental feature; do not use unless you know what you're doing"
        );
        let fetcher = TreeDataFetcher::new(main_node_client.clone(), connection_pool.clone())
            .with_l1_data(eth_client.clone(), config.remote.diamond_proxy_addr)?;
        app_health.insert_component(fetcher.health_check())?;
        task_handles.push(tokio::spawn(fetcher.run(stop_receiver.clone())));
    }

    let sync_state = if components.contains(&Component::Core) {
        run_core(
            config,
            connection_pool.clone(),
            main_node_client.clone(),
            eth_client,
            task_handles,
            app_health,
            stop_receiver.clone(),
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
        let fee_params_fetcher = Arc::new(MainNodeFeeParamsFetcher::new(main_node_client.clone()));
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

    Ok(())
}

async fn shutdown_components(
    tasks: ManagedTasks,
    healthcheck_handle: HealthCheckHandle,
) -> anyhow::Result<()> {
    task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .context("error waiting for RocksDB instances to drop")?;
    // Increase timeout because of complicated graceful shutdown procedure for API servers.
    tasks.complete(Duration::from_secs(30)).await;
    healthcheck_handle.stop().await;
    Ok(())
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Command {
    /// Generates consensus secret keys to use in the secrets file.
    /// Prints the keys to the stdout, you need to copy the relevant keys into your secrets file.
    GenerateSecrets,
}

/// External node for ZKsync Era.
#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Enables consensus-based syncing instead of JSON-RPC based one. This is an experimental and incomplete feature;
    /// do not use unless you know what you're doing.
    #[arg(long)]
    enable_consensus: bool,

    /// Comma-separated list of components to launch.
    #[arg(long, default_value = "all")]
    components: ComponentsToRun,
    /// Path to the yaml config. If set, it will be used instead of env vars.
    #[arg(
        long,
        requires = "secrets_path",
        requires = "external_node_config_path"
    )]
    config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with secrets. If set, it will be used instead of env vars.
    #[arg(long, requires = "config_path", requires = "external_node_config_path")]
    secrets_path: Option<std::path::PathBuf>,
    /// Path to the yaml with external node specific configuration. If set, it will be used instead of env vars.
    #[arg(long, requires = "config_path", requires = "secrets_path")]
    external_node_config_path: Option<std::path::PathBuf>,
    /// Path to the yaml with consensus config. If set, it will be used instead of env vars.
    #[arg(
        long,
        requires = "config_path",
        requires = "secrets_path",
        requires = "external_node_config_path",
        requires = "enable_consensus"
    )]
    consensus_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub enum Component {
    HttpApi,
    WsApi,
    Tree,
    TreeApi,
    TreeFetcher,
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
            "tree_fetcher" => Ok(&[Component::TreeFetcher]),
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

    if let Some(cmd) = &opt.command {
        match cmd {
            Command::GenerateSecrets => generate_consensus_secrets(),
        }
        return Ok(());
    }

    let mut config = if let Some(config_path) = opt.config_path.clone() {
        let secrets_path = opt.secrets_path.clone().unwrap();
        let external_node_config_path = opt.external_node_config_path.clone().unwrap();
        if opt.enable_consensus {
            anyhow::ensure!(
                opt.consensus_path.is_some(),
                "if --config-path and --enable-consensus are specified, then --consensus-path should be used to specify the location of the consensus config"
            );
        }
        ExternalNodeConfig::from_files(
            config_path,
            external_node_config_path,
            secrets_path,
            opt.consensus_path.clone(),
        )?
    } else {
        ExternalNodeConfig::new().context("Failed to load node configuration")?
    };

    if !opt.enable_consensus {
        config.consensus = None;
    }
    // Note: when old code will be removed, observability must be build within
    // tokio context.
    let guard = config.observability.build_observability()?;

    // Build L1 and L2 clients.
    let main_node_url = &config.required.main_node_url;
    tracing::info!("Main node URL is: {main_node_url:?}");
    let main_node_client = Client::http(main_node_url.clone())
        .context("failed creating JSON-RPC client for main node")?
        .for_network(config.required.l2_chain_id.into())
        .with_allowed_requests_per_second(config.optional.main_node_rate_limit_rps)
        .build();
    let main_node_client = Box::new(main_node_client) as Box<DynClient<L2>>;

    let eth_client_url = &config.required.eth_client_url;
    let eth_client = Client::http(eth_client_url.clone())
        .context("failed creating JSON-RPC client for Ethereum")?
        .for_network(config.required.settlement_layer_id().into())
        .build();
    let eth_client = Box::new(eth_client);

    let config = config
        .fetch_remote(main_node_client.as_ref())
        .await
        .context("failed fetching remote part of node config from main node")?;

    // Can be used to force the old approach to the external node.
    let force_old_approach = std::env::var("EXTERNAL_NODE_OLD_APPROACH").is_ok();

    // If the node framework is used, run the node.
    if !force_old_approach {
        // We run the node from a different thread, since the current thread is in tokio context.
        std::thread::spawn(move || {
            let node =
                ExternalNodeBuilder::new(config)?.build(opt.components.0.into_iter().collect())?;
            node.run(guard)?;
            anyhow::Ok(())
        })
        .join()
        .expect("Failed to run the node")?;

        return Ok(());
    }

    tracing::info!("Running the external node in the old approach");

    if let Some(threshold) = config.optional.slow_query_threshold() {
        ConnectionPool::<Core>::global_config().set_slow_query_threshold(threshold)?;
    }
    if let Some(threshold) = config.optional.long_connection_threshold() {
        ConnectionPool::<Core>::global_config().set_long_connection_threshold(threshold)?;
    }

    RUST_METRICS.initialize();
    EN_METRICS.observe_config(
        config.required.l1_chain_id,
        config.required.settlement_layer_id(),
        config.required.l2_chain_id,
        config.postgres.max_connections,
    );

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
    main_node_client: Box<DynClient<L2>>,
    eth_client: Box<DynClient<L1>>,
) -> anyhow::Result<()> {
    tracing::warn!("The external node is in the alpha phase, and should be used with caution.");
    tracing::info!("Started the external node");
    let (stop_sender, mut stop_receiver) = watch::channel(false);
    let stop_sender = Arc::new(stop_sender);

    let app_health = Arc::new(AppHealthCheck::new(
        config.optional.healthcheck_slow_time_limit(),
        config.optional.healthcheck_hard_time_limit(),
    ));
    app_health.insert_custom_component(Arc::new(MainNodeHealthCheck::from(
        main_node_client.clone(),
    )))?;

    // Start the health check server early into the node lifecycle so that its health can be monitored from the very start.
    let healthcheck_handle = HealthCheckHandle::spawn_server(
        ([0, 0, 0, 0], config.required.healthcheck_port).into(),
        app_health.clone(),
    );
    // Start exporting metrics at the very start so that e.g., snapshot recovery metrics are timely reported.
    let prometheus_task = if let Some(prometheus) = config.observability.prometheus() {
        tracing::info!("Starting Prometheus exporter with configuration: {prometheus:?}");

        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");
        app_health.insert_component(prometheus_health_check)?;
        let stop_receiver_for_exporter = stop_receiver.clone();
        Some(tokio::spawn(async move {
            prometheus_health_updater.update(HealthStatus::Ready.into());
            let result = prometheus.run(stop_receiver_for_exporter).await;
            drop(prometheus_health_updater);
            result
        }))
    } else {
        None
    };

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
        config.required.settlement_layer_id(),
        config.required.l2_chain_id,
        eth_client.clone(),
        main_node_client.clone(),
    );
    let validate_chain_ids_task = tokio::spawn(validate_chain_ids_task.run(stop_receiver.clone()));

    let mut task_handles = vec![metrics_task, validate_chain_ids_task];
    task_handles.extend(prometheus_task);

    // Make sure that the node storage is initialized either via genesis or snapshot recovery.
    let recovery_config =
        config
            .optional
            .snapshots_recovery_enabled
            .then_some(SnapshotRecoveryConfig {
                snapshot_l1_batch_override: config.experimental.snapshots_recovery_l1_batch,
                drop_storage_key_preimages: config
                    .experimental
                    .snapshots_recovery_drop_storage_key_preimages,
                object_store_config: config.optional.snapshots_recovery_object_store.clone(),
            });
    // Note: while stop receiver is passed there, it won't be respected, since we wait this task
    // to complete. Will be fixed after migration to the node framework.
    ensure_storage_initialized(
        stop_receiver.clone(),
        connection_pool.clone(),
        main_node_client.clone(),
        &app_health,
        config.required.l2_chain_id,
        recovery_config,
    )
    .await?;
    let sigint_receiver = env.setup_sigint_handler();
    // Spawn reacting to signals in a separate task so that the node is responsive to signals right away
    // (e.g., during the initial reorg detection).
    tokio::spawn({
        let stop_sender = stop_sender.clone();
        async move {
            sigint_receiver.await.ok();
            tracing::info!("Stop signal received, shutting down");
            stop_sender.send_replace(true);
        }
    });

    // Revert the storage if needed.
    let mut reverter = BlockReverter::new(NodeRole::External, connection_pool.clone());
    // Reverting executed batches is more-or-less safe for external nodes.
    let reverter = reverter
        .allow_rolling_back_executed_batches()
        .enable_rolling_back_postgres()
        .enable_rolling_back_merkle_tree(config.required.merkle_tree_path.clone())
        .add_rocksdb_storage_path_to_rollback(config.required.state_cache_path.clone());

    let mut reorg_detector = ReorgDetector::new(main_node_client.clone(), connection_pool.clone());
    // We're checking for the reorg in the beginning because we expect that if reorg is detected during
    // the node lifecycle, the node will exit the same way as it does with any other critical error,
    // and would restart. Then, on the 2nd launch reorg would be detected here, then processed and the node
    // will be able to operate normally afterwards.
    match reorg_detector.run_once(stop_receiver.clone()).await {
        Ok(()) if *stop_receiver.borrow() => {
            tracing::info!("Stop signal received during initial reorg detection; shutting down");
            healthcheck_handle.stop().await;
            return Ok(());
        }
        Ok(()) => {
            tracing::info!("Successfully checked no reorg compared to the main node");
        }
        Err(zksync_reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
            tracing::info!("Reverting to l1 batch number {last_correct_l1_batch}");
            reverter.roll_back(last_correct_l1_batch).await?;
            tracing::info!("Revert successfully completed");
        }
        Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
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
        // We don't want to log unnecessary warnings in `tasks.wait_single()` if we have received a stop signal.
        biased;

        _ = stop_receiver.changed() => {},
        () = tasks.wait_single() => {},
    }

    // Reaching this point means that either some actor exited unexpectedly or we received a stop signal.
    // Broadcast the stop signal (in case it wasn't broadcast previously) to all actors and exit.
    stop_sender.send_replace(true);
    shutdown_components(tasks, healthcheck_handle).await?;
    tracing::info!("Stopped");
    Ok(())
}
