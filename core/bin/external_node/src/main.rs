use prometheus_exporter::run_prometheus_exporter;
use tokio::{sync::watch, task, time::sleep};

use std::{sync::Arc, time::Duration};
use zksync_basic_types::L2ChainId;
use zksync_config::ZkSyncConfig;
use zksync_core::{
    api_server::{healthcheck, tx_sender::TxSenderBuilder, web3::ApiBuilder},
    block_reverter::{BlockReverter, BlockReverterFlags, L1ExecutedBatchesRevert},
    consistency_checker::ConsistencyChecker,
    data_fetchers::token_list::TokenListFetcher,
    l1_gas_price::MainNodeGasPriceFetcher,
    metadata_calculator::{MetadataCalculator, TreeImplementation},
    reorg_detector::ReorgDetector,
    setup_sigint_handler,
    state_keeper::{
        batch_executor::MainBatchExecutorBuilder, seal_criteria::SealManager, ZkSyncStateKeeper,
    },
    sync_layer::{
        batch_status_updater::run_batch_status_updater, external_io::ExternalIO,
        fetcher::MainNodeFetcher, genesis::perform_genesis_if_needed, ActionQueue,
        ExternalNodeSealer, SyncState,
    },
    wait_for_tasks,
};
use zksync_dal::{healthcheck::ConnectionPoolHealthCheck, ConnectionPool};
use zksync_health_check::CheckHealth;
use zksync_storage::RocksDB;

/// Creates the state keeper configured to work in the external node mode.
fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    main_node_url: String,
    connection_pool: ConnectionPool,
    sync_state: SyncState,
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

    let io = Box::new(ExternalIO::new(
        connection_pool,
        action_queue,
        sync_state,
        main_node_url,
    ));

    ZkSyncStateKeeper::new(stop_receiver, io, batch_executor_base, sealer)
}

async fn init_tasks(
    config: ZkSyncConfig,
    connection_pool: ConnectionPool,
) -> (Vec<task::JoinHandle<()>>, watch::Sender<bool>) {
    let main_node_url = config.api.web3_json_rpc.main_node_url.as_ref().unwrap();
    let (stop_sender, stop_receiver) = watch::channel::<bool>(false);
    let mut healthchecks: Vec<Box<dyn CheckHealth>> = Vec::new();
    // Create components.
    let gas_adjuster = Arc::new(MainNodeGasPriceFetcher::new(main_node_url));

    let sync_state = SyncState::new();
    let action_queue = ActionQueue::new();
    let state_keeper = build_state_keeper(
        action_queue.clone(),
        config.db.state_keeper_db_path.clone(),
        main_node_url.clone(),
        connection_pool.clone(),
        sync_state.clone(),
        stop_receiver.clone(),
    );
    let fetcher = MainNodeFetcher::new(
        ConnectionPool::new(Some(1), true),
        main_node_url,
        action_queue.clone(),
        sync_state.clone(),
        stop_receiver.clone(),
    );
    let metadata_calculator = MetadataCalculator::lightweight(&config, TreeImplementation::New);
    healthchecks.push(Box::new(metadata_calculator.tree_health_check()));
    let consistency_checker = ConsistencyChecker::new(
        &config.eth_client.web3_url,
        config.contracts.validator_timelock_addr,
        10,
        ConnectionPool::new(Some(1), true),
    );
    // We need this component to fetch "well-known" tokens.
    // And we need to know "well-known" tokens since there are paymaster-related
    // checks which depend on this particular token quality.
    let token_list_fetcher = TokenListFetcher::new(config.clone());

    // Run the components.
    let prometheus_task = run_prometheus_exporter(config.api.prometheus.clone(), false);
    let tree_stop_receiver = stop_receiver.clone();
    let tree_handle = task::spawn_blocking(move || {
        let pool = ConnectionPool::new(Some(1), true);
        metadata_calculator.run(&pool, tree_stop_receiver);
    });
    let consistency_checker_handle = tokio::spawn(consistency_checker.run(stop_receiver.clone()));
    let updater_stop_receiver = stop_receiver.clone();
    let updater_handle = task::spawn_blocking(move || {
        run_batch_status_updater(
            ConnectionPool::new(Some(1), true),
            action_queue,
            updater_stop_receiver,
        )
    });
    let sk_handle = task::spawn_blocking(|| state_keeper.run());
    let fetcher_handle = tokio::spawn(fetcher.run());
    let gas_adjuster_handle = tokio::spawn(gas_adjuster.clone().run(stop_receiver.clone()));

    let tx_sender = {
        let mut tx_sender_builder =
            TxSenderBuilder::new(config.clone().into(), connection_pool.clone())
                .with_main_connection_pool(connection_pool.clone())
                .with_tx_proxy(main_node_url.clone())
                .with_state_keeper_config(config.chain.state_keeper.clone());

        // Add rate limiter if enabled.
        if let Some(transactions_per_sec_limit) =
            config.api.web3_json_rpc.transactions_per_sec_limit
        {
            tx_sender_builder = tx_sender_builder.with_rate_limiter(transactions_per_sec_limit);
        };

        tx_sender_builder.build(gas_adjuster, config.chain.state_keeper.default_aa_hash)
    };

    let http_api_handle =
        ApiBuilder::jsonrpc_backend(config.clone().into(), connection_pool.clone())
            .http(config.api.web3_json_rpc.http_port)
            .with_filter_limit(config.api.web3_json_rpc.filters_limit())
            .with_threads(config.api.web3_json_rpc.threads_per_server as usize)
            .with_tx_sender(tx_sender.clone())
            .with_sync_state(sync_state.clone())
            .build(stop_receiver.clone());

    let token_list_fetcher_handle =
        tokio::spawn(token_list_fetcher.run(connection_pool.clone(), stop_receiver.clone()));

    let mut task_handles = ApiBuilder::jsonrpc_backend(config.clone().into(), connection_pool)
        .ws(config.api.web3_json_rpc.ws_port)
        .with_filter_limit(config.api.web3_json_rpc.filters_limit())
        .with_subscriptions_limit(config.api.web3_json_rpc.subscriptions_limit())
        .with_polling_interval(config.api.web3_json_rpc.pubsub_interval())
        .with_tx_sender(tx_sender)
        .with_sync_state(sync_state)
        .build(stop_receiver.clone());

    healthchecks.push(Box::new(ConnectionPoolHealthCheck::new(
        ConnectionPool::new(Some(1), true),
    )));
    let healthcheck_handle = healthcheck::start_server_thread_detached(
        config.api.healthcheck.bind_addr(),
        healthchecks,
        stop_receiver,
    );

    task_handles.extend(http_api_handle);
    task_handles.extend([
        prometheus_task,
        sk_handle,
        fetcher_handle,
        updater_handle,
        tree_handle,
        gas_adjuster_handle,
        consistency_checker_handle,
        healthcheck_handle,
        token_list_fetcher_handle,
    ]);

    (task_handles, stop_sender)
}

async fn shutdown_components(stop_sender: watch::Sender<bool>) {
    let _ = stop_sender.send(true);
    RocksDB::await_rocksdb_termination();
    // Sleep for some time to let components gracefully stop.
    sleep(Duration::from_secs(10)).await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial setup.

    let _sentry_guard = vlog::init();
    let connection_pool = ConnectionPool::new(None, true);
    let config = ZkSyncConfig::from_env();
    let main_node_url = config.api.web3_json_rpc.main_node_url.as_ref().unwrap();
    let sigint_receiver = setup_sigint_handler();

    vlog::info!("Started the external node");
    vlog::info!("Main node URL is: {}", main_node_url);

    // Make sure that genesis is performed.
    perform_genesis_if_needed(
        &mut connection_pool.access_storage_blocking(),
        L2ChainId(config.chain.eth.zksync_network_id),
        config.chain.state_keeper.base_system_contracts_hashes(),
        main_node_url.clone(),
    )
    .await;

    let (task_handles, stop_sender) = init_tasks(config.clone(), connection_pool.clone()).await;

    let reorg_detector = ReorgDetector::new(main_node_url, connection_pool.clone());
    let reorg_detector_handle = tokio::spawn(reorg_detector.run());

    tokio::select! {
        _ = wait_for_tasks(task_handles, false) => {},
        _ = sigint_receiver => {
            vlog::info!("Stop signal received, shutting down");
        },
        last_correct_batch = reorg_detector_handle => {
            if let Ok(last_correct_batch) = last_correct_batch {
                vlog::info!("Performing rollback to block {}", last_correct_batch);
                shutdown_components(stop_sender).await;
                BlockReverter::new(config, connection_pool, L1ExecutedBatchesRevert::Allowed)
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
    shutdown_components(stop_sender).await;
    Ok(())
}
