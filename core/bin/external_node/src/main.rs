use prometheus_exporter::run_prometheus_exporter;
use tokio::{sync::watch, task, time::sleep};
use zksync_state::FactoryDepsCache;

use config::ExternalNodeConfig;
use std::{sync::Arc, time::Duration};
use zksync_basic_types::Address;
use zksync_config::DBConfig;

use zksync_core::api_server::healthcheck::HealthCheckHandle;
use zksync_core::{
    api_server::{
        execution_sandbox::VmConcurrencyLimiter, healthcheck, tx_sender::TxSenderBuilder,
        web3::ApiBuilder,
    },
    block_reverter::{BlockReverter, BlockReverterFlags, L1ExecutedBatchesRevert},
    consistency_checker::ConsistencyChecker,
    l1_gas_price::MainNodeGasPriceFetcher,
    metadata_calculator::{
        MetadataCalculator, MetadataCalculatorConfig, MetadataCalculatorModeConfig,
    },
    reorg_detector::ReorgDetector,
    setup_sigint_handler,
    state_keeper::{MainBatchExecutorBuilder, SealManager, ZkSyncStateKeeper},
    sync_layer::{
        batch_status_updater::BatchStatusUpdater, external_io::ExternalIO,
        fetcher::MainNodeFetcher, genesis::perform_genesis_if_needed, ActionQueue,
        ExternalNodeSealer, SyncState,
    },
};
use zksync_dal::{connection::DbVariant, healthcheck::ConnectionPoolHealthCheck, ConnectionPool};
use zksync_health_check::CheckHealth;
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::wait_for_tasks;

mod config;

/// Creates the state keeper configured to work in the external node mode.
async fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    main_node_url: String,
    connection_pool: ConnectionPool,
    sync_state: SyncState,
    l2_erc20_bridge_addr: Address,
    stop_receiver: watch::Receiver<bool>,
) -> ZkSyncStateKeeper {
    let en_sealer = ExternalNodeSealer::new(action_queue.clone());
    let sealer = SealManager::custom(
        None,
        vec![en_sealer.clone().into_unconditional_batch_seal_criterion()],
        vec![en_sealer.into_miniblock_seal_criterion()],
    );

    // These config values are used on the main node, and depending on these values certain transactions can
    // be *rejected* (that is, not included into the block). However, external node only mirrors what the main
    // node has already executed, so we can safely set these values to the maximum possible values - if the main
    // node has already executed the transaction, then the external node must execute it too.
    let max_allowed_l2_tx_gas_limit = u32::MAX.into();
    let validation_computational_gas_limit = u32::MAX;
    // We don't need call traces on the external node.
    let save_call_traces = false;

    let batch_executor_base: Box<MainBatchExecutorBuilder> =
        Box::new(MainBatchExecutorBuilder::new(
            state_keeper_db_path,
            connection_pool.clone(),
            max_allowed_l2_tx_gas_limit,
            save_call_traces,
            validation_computational_gas_limit,
        ));

    let io = Box::new(
        ExternalIO::new(
            connection_pool,
            action_queue,
            sync_state,
            main_node_url,
            l2_erc20_bridge_addr,
        )
        .await,
    );

    ZkSyncStateKeeper::new(stop_receiver, io, batch_executor_base, sealer)
}

async fn init_tasks(
    config: ExternalNodeConfig,
    connection_pool: ConnectionPool,
) -> (
    Vec<task::JoinHandle<()>>,
    watch::Sender<bool>,
    HealthCheckHandle,
) {
    let main_node_url = config
        .required
        .main_node_url()
        .expect("Main node URL is incorrect");
    let (stop_sender, stop_receiver) = watch::channel::<bool>(false);
    let mut healthchecks: Vec<Box<dyn CheckHealth>> = Vec::new();
    // Create components.
    let gas_adjuster = Arc::new(MainNodeGasPriceFetcher::new(&main_node_url));

    let sync_state = SyncState::new();
    let action_queue = ActionQueue::new();
    let state_keeper = build_state_keeper(
        action_queue.clone(),
        config.required.state_cache_path.clone(),
        main_node_url.to_string(),
        connection_pool.clone(),
        sync_state.clone(),
        config.remote.l2_erc20_bridge_addr,
        stop_receiver.clone(),
    )
    .await;
    let fetcher = MainNodeFetcher::new(
        ConnectionPool::new(Some(1), DbVariant::Master).await,
        &main_node_url,
        action_queue.clone(),
        sync_state.clone(),
        stop_receiver.clone(),
    )
    .await;

    let metadata_calculator = MetadataCalculator::new(&MetadataCalculatorConfig {
        db_path: &config.required.merkle_tree_path,
        mode: MetadataCalculatorModeConfig::Lightweight,
        delay_interval: config.optional.metadata_calculator_delay(),
        max_block_batch: config.optional.max_blocks_per_tree_batch(),
        throttle_interval: config.optional.merkle_tree_throttle(),
    })
    .await;
    healthchecks.push(Box::new(metadata_calculator.tree_health_check()));

    let consistency_checker = ConsistencyChecker::new(
        &config
            .required
            .eth_client_url()
            .expect("L1 client URL is incorrect"),
        10,
        ConnectionPool::new(Some(1), DbVariant::Master).await,
    );

    let batch_status_updater = BatchStatusUpdater::new(
        &main_node_url,
        ConnectionPool::new(Some(1), DbVariant::Master).await,
    )
    .await;

    // Run the components.
    let tree_stop_receiver = stop_receiver.clone();
    let tree_pool = ConnectionPool::new(Some(1), DbVariant::Master).await;
    let prover_tree_pool = ConnectionPool::new(Some(1), DbVariant::Prover).await;
    let tree_handle =
        task::spawn(metadata_calculator.run(tree_pool, prover_tree_pool, tree_stop_receiver));
    let consistency_checker_handle = tokio::spawn(consistency_checker.run(stop_receiver.clone()));
    let updater_handle = task::spawn(batch_status_updater.run(stop_receiver.clone()));
    let sk_handle = task::spawn(state_keeper.run());
    let fetcher_handle = tokio::spawn(fetcher.run());
    let gas_adjuster_handle = tokio::spawn(gas_adjuster.clone().run(stop_receiver.clone()));

    let tx_sender = {
        let mut tx_sender_builder =
            TxSenderBuilder::new(config.clone().into(), connection_pool.clone())
                .with_main_connection_pool(connection_pool.clone())
                .with_tx_proxy(&main_node_url);

        // Add rate limiter if enabled.
        if let Some(tps_limit) = config.optional.transactions_per_sec_limit {
            tx_sender_builder = tx_sender_builder.with_rate_limiter(tps_limit);
        };

        let vm_concurrency_limiter =
            VmConcurrencyLimiter::new(config.optional.vm_concurrency_limit());

        let factory_deps_cache = FactoryDepsCache::new(
            "factory_deps_cache",
            config.optional.factory_deps_cache_size_mb(),
        );

        tx_sender_builder
            .build(
                gas_adjuster,
                config.required.default_aa_hash,
                Arc::new(vm_concurrency_limiter),
                factory_deps_cache.clone(),
            )
            .await
    };

    let (http_api_handle, http_api_healthcheck) =
        ApiBuilder::jsonrpc_backend(config.clone().into(), connection_pool.clone())
            .http(config.required.http_port)
            .with_filter_limit(config.optional.filters_limit())
            .with_threads(config.required.threads_per_server)
            .with_tx_sender(tx_sender.clone())
            .with_sync_state(sync_state.clone())
            .build(stop_receiver.clone())
            .await;

    let (mut task_handles, ws_api_healthcheck) =
        ApiBuilder::jsonrpc_backend(config.clone().into(), connection_pool)
            .ws(config.required.ws_port)
            .with_filter_limit(config.optional.filters_limit())
            .with_subscriptions_limit(config.optional.subscriptions_limit())
            .with_polling_interval(config.optional.polling_interval())
            .with_threads(config.required.threads_per_server)
            .with_tx_sender(tx_sender)
            .with_sync_state(sync_state)
            .build(stop_receiver.clone())
            .await;

    healthchecks.push(Box::new(ws_api_healthcheck));
    healthchecks.push(Box::new(http_api_healthcheck));
    healthchecks.push(Box::new(ConnectionPoolHealthCheck::new(
        ConnectionPool::new(Some(1), DbVariant::Master).await,
    )));
    let healthcheck_handle = healthcheck::start_server_thread_detached(
        ([0, 0, 0, 0], config.required.healthcheck_port).into(),
        healthchecks,
    );
    if let Some(port) = config.optional.prometheus_port {
        let prometheus_task = run_prometheus_exporter(port, None);
        task_handles.push(prometheus_task);
    }

    task_handles.extend(http_api_handle);
    task_handles.extend([
        sk_handle,
        fetcher_handle,
        updater_handle,
        tree_handle,
        gas_adjuster_handle,
        consistency_checker_handle,
    ]);

    (task_handles, stop_sender, healthcheck_handle)
}

async fn shutdown_components(
    stop_sender: watch::Sender<bool>,
    healthcheck_handle: HealthCheckHandle,
) {
    let _ = stop_sender.send(true);
    RocksDB::await_rocksdb_termination();
    // Sleep for some time to let components gracefully stop.
    sleep(Duration::from_secs(10)).await;
    healthcheck_handle.stop().await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial setup.

    vlog::init();
    let _sentry_guard = vlog::init_sentry();
    let config = ExternalNodeConfig::collect()
        .await
        .expect("Failed to load external node config");
    let main_node_url = config
        .required
        .main_node_url()
        .expect("Main node URL is incorrect");

    let connection_pool = ConnectionPool::new(None, DbVariant::Master).await;
    let sigint_receiver = setup_sigint_handler();

    vlog::warn!("The external node is in the alpha phase, and should be used with caution.");

    vlog::info!("Started the external node");
    vlog::info!("Main node URL is: {}", main_node_url);

    // Make sure that genesis is performed.
    perform_genesis_if_needed(
        &mut connection_pool.access_storage().await,
        config.remote.l2_chain_id,
        config.base_system_contracts_hashes(),
        main_node_url.clone(),
    )
    .await;

    let (task_handles, stop_sender, health_check_handle) =
        init_tasks(config.clone(), connection_pool.clone()).await;

    let reorg_detector = ReorgDetector::new(&main_node_url, connection_pool.clone());
    let reorg_detector_handle = tokio::spawn(reorg_detector.run());

    let reverter_config = DBConfig {
        state_keeper_db_path: config.required.state_cache_path.clone(),
        new_merkle_tree_ssd_path: config.required.merkle_tree_path.clone(),
        ..Default::default()
    };

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(task_handles, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = sigint_receiver => {
            vlog::info!("Stop signal received, shutting down");
        },
        last_correct_batch = reorg_detector_handle => {
            if let Ok(last_correct_batch) = last_correct_batch {
                vlog::info!("Performing rollback to block {}", last_correct_batch);
                shutdown_components(stop_sender, health_check_handle).await;
                BlockReverter::new(reverter_config, None, connection_pool, L1ExecutedBatchesRevert::Allowed)
                    .rollback_db(last_correct_batch, BlockReverterFlags::all())
                    .await;
                vlog::info!("Rollback successfully completed, the node has to restart to continue working");
                return Ok(());
            } else {
                vlog::error!("Reorg detector actor failed");
            }
        }
    }

    // Reaching this point means that either some actor exited unexpectedly or we received a stop signal.
    // Broadcast the stop signal to all actors and exit.
    shutdown_components(stop_sender, health_check_handle).await;
    Ok(())
}
