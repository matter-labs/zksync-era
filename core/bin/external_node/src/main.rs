use anyhow::Context;
use clap::Parser;
use tokio::{sync::watch, task, time::sleep};

use std::{sync::Arc, time::Duration};

use futures::{future::FusedFuture, FutureExt};
use prometheus_exporter::PrometheusExporterConfig;
use zksync_basic_types::{Address, L2ChainId};
use zksync_core::{
    api_server::{
        execution_sandbox::VmConcurrencyLimiter,
        healthcheck::HealthCheckHandle,
        tx_sender::{ApiContracts, TxSenderBuilder},
        web3::{ApiBuilder, Namespace},
    },
    block_reverter::{BlockReverter, BlockReverterFlags, L1ExecutedBatchesRevert},
    consistency_checker::ConsistencyChecker,
    l1_gas_price::MainNodeGasPriceFetcher,
    metadata_calculator::{
        MetadataCalculator, MetadataCalculatorConfig, MetadataCalculatorModeConfig,
    },
    reorg_detector::ReorgDetector,
    setup_sigint_handler,
    state_keeper::{L1BatchExecutorBuilder, MainBatchExecutorBuilder, ZkSyncStateKeeper},
    sync_layer::{
        batch_status_updater::BatchStatusUpdater, external_io::ExternalIO,
        fetcher::MainNodeFetcherCursor, genesis::perform_genesis_if_needed, ActionQueue,
        MainNodeClient, SyncState,
    },
};
use zksync_dal::{healthcheck::ConnectionPoolHealthCheck, ConnectionPool};
use zksync_health_check::CheckHealth;
use zksync_state::PostgresStorageCaches;
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::wait_for_tasks;

mod config;

use crate::config::ExternalNodeConfig;

/// Creates the state keeper configured to work in the external node mode.
#[allow(clippy::too_many_arguments)]
async fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool,
    sync_state: SyncState,
    l2_erc20_bridge_addr: Address,
    stop_receiver: watch::Receiver<bool>,
    chain_id: L2ChainId,
) -> ZkSyncStateKeeper {
    // These config values are used on the main node, and depending on these values certain transactions can
    // be *rejected* (that is, not included into the block). However, external node only mirrors what the main
    // node has already executed, so we can safely set these values to the maximum possible values - if the main
    // node has already executed the transaction, then the external node must execute it too.
    let max_allowed_l2_tx_gas_limit = u32::MAX.into();
    let validation_computational_gas_limit = u32::MAX;
    // We only need call traces on the external node if the `debug_` namespace is enabled.
    let save_call_traces = config.optional.api_namespaces().contains(&Namespace::Debug);

    let batch_executor_base: Box<dyn L1BatchExecutorBuilder> =
        Box::new(MainBatchExecutorBuilder::new(
            state_keeper_db_path,
            connection_pool.clone(),
            max_allowed_l2_tx_gas_limit,
            save_call_traces,
            false,
            config.optional.enum_index_migration_chunk_size,
        ));

    let main_node_url = config.required.main_node_url().unwrap();
    let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
        .expect("Failed creating JSON-RPC client for main node");
    let io = ExternalIO::new(
        connection_pool,
        action_queue,
        sync_state,
        Box::new(main_node_client),
        l2_erc20_bridge_addr,
        validation_computational_gas_limit,
        chain_id,
    )
    .await;
    io.recalculate_miniblock_hashes().await;

    ZkSyncStateKeeper::without_sealer(stop_receiver, Box::new(io), batch_executor_base)
}

async fn init_tasks(
    config: ExternalNodeConfig,
    connection_pool: ConnectionPool,
) -> anyhow::Result<(
    Vec<task::JoinHandle<anyhow::Result<()>>>,
    watch::Sender<bool>,
    HealthCheckHandle,
    watch::Receiver<bool>,
)> {
    let main_node_url = config
        .required
        .main_node_url()
        .expect("Main node URL is incorrect");
    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut healthchecks: Vec<Box<dyn CheckHealth>> = Vec::new();
    // Create components.
    let gas_adjuster = Arc::new(MainNodeGasPriceFetcher::new(&main_node_url));

    let sync_state = SyncState::new();
    let (action_queue_sender, action_queue) = ActionQueue::new();
    let state_keeper = build_state_keeper(
        action_queue,
        config.required.state_cache_path.clone(),
        &config,
        connection_pool.clone(),
        sync_state.clone(),
        config.remote.l2_erc20_bridge_addr,
        stop_receiver.clone(),
        config.remote.l2_chain_id,
    )
    .await;

    let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
        .context("Failed creating JSON-RPC client for main node")?;
    let singleton_pool_builder = ConnectionPool::singleton(&config.postgres.database_url);
    let fetcher_cursor = {
        let pool = singleton_pool_builder
            .build()
            .await
            .context("failed to build a connection pool for `MainNodeFetcher`")?;
        let mut storage = pool.access_storage_tagged("sync_layer").await?;
        MainNodeFetcherCursor::new(&mut storage)
            .await
            .context("failed to load `MainNodeFetcher` cursor from Postgres")?
    };
    let fetcher = fetcher_cursor.into_fetcher(
        Box::new(main_node_client),
        action_queue_sender,
        sync_state.clone(),
        stop_receiver.clone(),
    );

    let metadata_calculator = MetadataCalculator::new(&MetadataCalculatorConfig {
        db_path: &config.required.merkle_tree_path,
        mode: MetadataCalculatorModeConfig::Full {
            store_factory: None,
        },
        delay_interval: config.optional.metadata_calculator_delay(),
        max_l1_batches_per_iter: config.optional.max_l1_batches_per_tree_iter,
        multi_get_chunk_size: config.optional.merkle_tree_multi_get_chunk_size,
        block_cache_capacity: config.optional.merkle_tree_block_cache_size(),
        memtable_capacity: config.optional.merkle_tree_memtable_capacity(),
        stalled_writes_timeout: config.optional.merkle_tree_stalled_writes_timeout(),
    })
    .await;
    healthchecks.push(Box::new(metadata_calculator.tree_health_check()));

    let consistency_checker = ConsistencyChecker::new(
        &config
            .required
            .eth_client_url()
            .context("L1 client URL is incorrect")?,
        10, // TODO (BFT-97): Make it a part of a proper EN config
        singleton_pool_builder
            .build()
            .await
            .context("failed to build connection pool for ConsistencyChecker")?,
    );

    let batch_status_updater = BatchStatusUpdater::new(
        &main_node_url,
        singleton_pool_builder
            .build()
            .await
            .context("failed to build a connection pool for BatchStatusUpdater")?,
    )
    .await;

    // Run the components.
    let tree_stop_receiver = stop_receiver.clone();
    let tree_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a tree_pool")?;
    // todo: PLA-335
    // Note: This pool isn't actually used by the metadata calculator, but it has to be provided anyway.
    let prover_tree_pool = ConnectionPool::singleton(&config.postgres.database_url)
        .build()
        .await
        .context("failed to build a prover_tree_pool")?;
    let tree_handle =
        task::spawn(metadata_calculator.run(tree_pool, prover_tree_pool, tree_stop_receiver));

    let consistency_checker_handle = tokio::spawn(consistency_checker.run(stop_receiver.clone()));

    let updater_handle = task::spawn(batch_status_updater.run(stop_receiver.clone()));
    let sk_handle = task::spawn(state_keeper.run());
    let fetcher_handle = tokio::spawn(fetcher.run());
    let gas_adjuster_handle = tokio::spawn(gas_adjuster.clone().run(stop_receiver.clone()));

    let (tx_sender, vm_barrier, cache_update_handle) = {
        let mut tx_sender_builder =
            TxSenderBuilder::new(config.clone().into(), connection_pool.clone())
                .with_main_connection_pool(connection_pool.clone())
                .with_tx_proxy(&main_node_url);

        // Add rate limiter if enabled.
        if let Some(tps_limit) = config.optional.transactions_per_sec_limit {
            tx_sender_builder = tx_sender_builder.with_rate_limiter(tps_limit);
        };

        let max_concurrency = config.optional.vm_concurrency_limit;
        let (vm_concurrency_limiter, vm_barrier) = VmConcurrencyLimiter::new(max_concurrency);
        let mut storage_caches = PostgresStorageCaches::new(
            config.optional.factory_deps_cache_size() as u64,
            config.optional.initial_writes_cache_size() as u64,
        );
        let latest_values_cache_size = config.optional.latest_values_cache_size() as u64;
        let cache_update_handle = (latest_values_cache_size > 0).then(|| {
            task::spawn_blocking(storage_caches.configure_storage_values_cache(
                latest_values_cache_size,
                connection_pool.clone(),
                tokio::runtime::Handle::current(),
            ))
        });

        let tx_sender = tx_sender_builder
            .build(
                gas_adjuster,
                Arc::new(vm_concurrency_limiter),
                ApiContracts::load_from_disk(), // TODO (BFT-138): Allow to dynamically reload API contracts
                storage_caches,
            )
            .await;
        (tx_sender, vm_barrier, cache_update_handle)
    };

    let http_server_handles =
        ApiBuilder::jsonrpc_backend(config.clone().into(), connection_pool.clone())
            .http(config.required.http_port)
            .with_filter_limit(config.optional.filters_limit)
            .with_batch_request_size_limit(config.optional.max_batch_request_size)
            .with_response_body_size_limit(config.optional.max_response_body_size())
            .with_threads(config.required.threads_per_server)
            .with_tx_sender(tx_sender.clone(), vm_barrier.clone())
            .with_sync_state(sync_state.clone())
            .enable_api_namespaces(config.optional.api_namespaces())
            .build(stop_receiver.clone())
            .await
            .context("Failed initializing HTTP JSON-RPC server")?;

    let ws_server_handles =
        ApiBuilder::jsonrpc_backend(config.clone().into(), connection_pool.clone())
            .ws(config.required.ws_port)
            .with_filter_limit(config.optional.filters_limit)
            .with_subscriptions_limit(config.optional.subscriptions_limit)
            .with_batch_request_size_limit(config.optional.max_batch_request_size)
            .with_response_body_size_limit(config.optional.max_response_body_size())
            .with_polling_interval(config.optional.polling_interval())
            .with_threads(config.required.threads_per_server)
            .with_tx_sender(tx_sender, vm_barrier)
            .with_sync_state(sync_state)
            .enable_api_namespaces(config.optional.api_namespaces())
            .build(stop_receiver.clone())
            .await
            .context("Failed initializing WS JSON-RPC server")?;

    healthchecks.push(Box::new(ws_server_handles.health_check));
    healthchecks.push(Box::new(http_server_handles.health_check));
    healthchecks.push(Box::new(ConnectionPoolHealthCheck::new(connection_pool)));
    let healthcheck_handle = HealthCheckHandle::spawn_server(
        ([0, 0, 0, 0], config.required.healthcheck_port).into(),
        healthchecks,
    );

    let mut task_handles = vec![];
    if let Some(port) = config.optional.prometheus_port {
        let prometheus_task = PrometheusExporterConfig::pull(port).run(stop_receiver.clone());
        task_handles.push(tokio::spawn(prometheus_task));
    }
    task_handles.extend(http_server_handles.tasks);
    task_handles.extend(ws_server_handles.tasks);
    task_handles.extend(cache_update_handle);
    task_handles.extend([
        sk_handle,
        fetcher_handle,
        updater_handle,
        tree_handle,
        gas_adjuster_handle,
    ]);
    task_handles.push(consistency_checker_handle);

    Ok((task_handles, stop_sender, healthcheck_handle, stop_receiver))
}

async fn shutdown_components(
    stop_sender: watch::Sender<bool>,
    healthcheck_handle: HealthCheckHandle,
) {
    stop_sender.send(true).ok();
    task::spawn_blocking(RocksDB::await_rocksdb_termination)
        .await
        .unwrap();
    // Sleep for some time to let components gracefully stop.
    sleep(Duration::from_secs(10)).await;
    healthcheck_handle.stop().await;
}

#[derive(Debug, Parser)]
#[structopt(author = "Matter Labs", version)]
struct Cli {
    #[arg(long)]
    revert_pending_l1_batch: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial setup.
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

    let config = ExternalNodeConfig::collect()
        .await
        .context("Failed to load external node config")?;
    let main_node_url = config
        .required
        .main_node_url()
        .context("Main node URL is incorrect")?;

    let connection_pool = ConnectionPool::builder(
        &config.postgres.database_url,
        config.postgres.max_connections,
    )
    .build()
    .await
    .context("failed to build a connection_pool")?;

    if opt.revert_pending_l1_batch {
        tracing::info!("Rolling pending L1 batch back..");
        let reverter = BlockReverter::new(
            config.required.state_cache_path,
            config.required.merkle_tree_path,
            None,
            connection_pool.clone(),
            L1ExecutedBatchesRevert::Allowed,
        );

        let mut connection = connection_pool.access_storage().await.unwrap();
        let sealed_l1_batch_number = connection
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap();
        drop(connection);

        tracing::info!("Rolling back to l1 batch number {sealed_l1_batch_number}");
        reverter
            .rollback_db(sealed_l1_batch_number, BlockReverterFlags::all())
            .await;
        tracing::info!(
            "Rollback successfully completed, the node has to restart to continue working"
        );
        return Ok(());
    }

    let sigint_receiver = setup_sigint_handler();

    tracing::warn!("The external node is in the alpha phase, and should be used with caution.");

    tracing::info!("Started the external node");
    tracing::info!("Main node URL is: {}", main_node_url);

    // Make sure that genesis is performed.
    let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
        .context("Failed creating JSON-RPC client for main node")?;
    perform_genesis_if_needed(
        &mut connection_pool.access_storage().await.unwrap(),
        config.remote.l2_chain_id,
        &main_node_client,
    )
    .await
    .context("Performing genesis failed")?;

    let (task_handles, stop_sender, health_check_handle, stop_receiver) =
        init_tasks(config.clone(), connection_pool.clone())
            .await
            .context("init_tasks")?;

    let reorg_detector = ReorgDetector::new(&main_node_url, connection_pool.clone(), stop_receiver);
    let mut reorg_detector_handle = tokio::spawn(reorg_detector.run()).fuse();

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    let mut reorg_detector_last_correct_batch = None;

    tokio::select! {
        _ = wait_for_tasks(task_handles, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = sigint_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
        last_correct_batch = &mut reorg_detector_handle => {
            if let Ok(last_correct_batch) = last_correct_batch {
                reorg_detector_last_correct_batch = last_correct_batch;
            } else {
                tracing::error!("Reorg detector actor failed");
            }
        }
    };

    // Reaching this point means that either some actor exited unexpectedly or we received a stop signal.
    // Broadcast the stop signal to all actors and exit.
    shutdown_components(stop_sender, health_check_handle).await;

    if !reorg_detector_handle.is_terminated() {
        if let Ok(Some(last_correct_batch)) = reorg_detector_handle.await {
            reorg_detector_last_correct_batch = Some(last_correct_batch);
        }
    }

    if let Some(last_correct_batch) = reorg_detector_last_correct_batch {
        tracing::info!("Performing rollback to block {}", last_correct_batch);
        let reverter = BlockReverter::new(
            config.required.state_cache_path,
            config.required.merkle_tree_path,
            None,
            connection_pool,
            L1ExecutedBatchesRevert::Allowed,
        );
        reverter
            .rollback_db(last_correct_batch, BlockReverterFlags::all())
            .await;
        tracing::info!(
            "Rollback successfully completed, the node has to restart to continue working"
        );
    }

    Ok(())
}
