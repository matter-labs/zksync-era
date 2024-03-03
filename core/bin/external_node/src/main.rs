use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use futures::{future::FusedFuture, FutureExt as _};
use metrics::EN_METRICS;
use prometheus_exporter::PrometheusExporterConfig;
use tokio::{sync::watch, task, time::sleep};
use zksync_basic_types::{Address, L2ChainId};
use zksync_concurrency::{ctx, scope};
use zksync_config::configs::database::MerkleTreeMode;
use zksync_core::{
    api_server::{
        execution_sandbox::VmConcurrencyLimiter,
        healthcheck::HealthCheckHandle,
        tx_sender::{proxy::TxProxy, ApiContracts, TxSenderBuilder},
        web3::{ApiBuilder, Namespace},
    },
    block_reverter::{BlockReverter, BlockReverterFlags, L1ExecutedBatchesRevert},
    commitment_generator::CommitmentGenerator,
    consensus,
    consistency_checker::ConsistencyChecker,
    l1_gas_price::MainNodeFeeParamsFetcher,
    metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig},
    reorg_detector::ReorgDetector,
    setup_sigint_handler,
    state_keeper::{
        seal_criteria::NoopSealer, BatchExecutor, MainBatchExecutor, MiniblockSealer,
        MiniblockSealerHandle, ZkSyncStateKeeper,
    },
    sync_layer::{
        batch_status_updater::BatchStatusUpdater, external_io::ExternalIO,
        fetcher::MainNodeFetcher, ActionQueue, MainNodeClient, SyncState,
    },
};
use zksync_dal::{healthcheck::ConnectionPoolHealthCheck, ConnectionPool};
use zksync_health_check::{AppHealthCheck, HealthStatus, ReactiveHealthCheck};
use zksync_l1_contract_interface::i_executor::commit::kzg::KzgSettings;
use zksync_state::PostgresStorageCaches;
use zksync_storage::RocksDB;
use zksync_utils::wait_for_tasks::wait_for_tasks;
use zksync_web3_decl::jsonrpsee::http_client::HttpClient;

use crate::{
    config::{observability::observability_config_from_env, ExternalNodeConfig},
    helpers::MainNodeHealthCheck,
    init::ensure_storage_initialized,
};

mod config;
mod helpers;
mod init;
mod metrics;

const RELEASE_MANIFEST: &str = include_str!("../../../../.github/release-please/manifest.json");

/// Creates the state keeper configured to work in the external node mode.
#[allow(clippy::too_many_arguments)]
async fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool,
    sync_state: SyncState,
    l2_erc20_bridge_addr: Address,
    miniblock_sealer_handle: MiniblockSealerHandle,
    stop_receiver: watch::Receiver<bool>,
    chain_id: L2ChainId,
) -> anyhow::Result<ZkSyncStateKeeper> {
    // These config values are used on the main node, and depending on these values certain transactions can
    // be *rejected* (that is, not included into the block). However, external node only mirrors what the main
    // node has already executed, so we can safely set these values to the maximum possible values - if the main
    // node has already executed the transaction, then the external node must execute it too.
    let max_allowed_l2_tx_gas_limit = u32::MAX.into();
    let validation_computational_gas_limit = u32::MAX;
    // We only need call traces on the external node if the `debug_` namespace is enabled.
    let save_call_traces = config.optional.api_namespaces().contains(&Namespace::Debug);

    let batch_executor_base: Box<dyn BatchExecutor> = Box::new(MainBatchExecutor::new(
        state_keeper_db_path,
        connection_pool.clone(),
        max_allowed_l2_tx_gas_limit,
        save_call_traces,
        false,
        config.optional.enum_index_migration_chunk_size,
        true,
    ));

    let main_node_url = config.required.main_node_url()?;
    let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
        .context("Failed creating JSON-RPC client for main node")?;
    let io = ExternalIO::new(
        miniblock_sealer_handle,
        connection_pool,
        action_queue,
        sync_state,
        Box::new(main_node_client),
        l2_erc20_bridge_addr,
        validation_computational_gas_limit,
        chain_id,
    )
    .await
    .context("Failed initializing I/O for external node state keeper")?;

    Ok(ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        batch_executor_base,
        Arc::new(NoopSealer),
    ))
}

async fn init_tasks(
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool,
    main_node_client: HttpClient,
    task_handles: &mut Vec<task::JoinHandle<anyhow::Result<()>>>,
    app_health: &AppHealthCheck,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let release_manifest: serde_json::Value = serde_json::from_str(RELEASE_MANIFEST)
        .expect("release manifest is a valid json document; qed");
    let release_manifest_version = release_manifest["core"].as_str().expect(
        "a release-please manifest with \"core\" version field was specified at build time; qed.",
    );

    let version = semver::Version::parse(release_manifest_version)
        .expect("version in manifest is a correct semver format; qed");
    // Create components.
    let fee_params_fetcher = Arc::new(MainNodeFeeParamsFetcher::new(main_node_client.clone()));

    let sync_state = SyncState::default();
    app_health.insert_custom_component(Arc::new(sync_state.clone()));
    let (action_queue_sender, action_queue) = ActionQueue::new();

    let (miniblock_sealer, miniblock_sealer_handle) = MiniblockSealer::new(
        connection_pool.clone(),
        config.optional.miniblock_seal_queue_capacity,
    );
    task_handles.push(tokio::spawn(miniblock_sealer.run()));
    let pool = connection_pool.clone();
    task_handles.push(tokio::spawn(async move {
        loop {
            let protocol_version = pool
                .access_storage()
                .await
                .unwrap()
                .protocol_versions_dal()
                .last_used_version_id()
                .await
                .map(|version| version as u16);

            EN_METRICS.version[&(format!("{}", version), protocol_version)].set(1);

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }));

    let state_keeper = build_state_keeper(
        action_queue,
        config.required.state_cache_path.clone(),
        config,
        connection_pool.clone(),
        sync_state.clone(),
        config.remote.l2_erc20_bridge_addr,
        miniblock_sealer_handle,
        stop_receiver.clone(),
        config.remote.l2_chain_id,
    )
    .await?;

    let singleton_pool_builder = ConnectionPool::singleton(&config.postgres.database_url);

    let fetcher_handle = if let Some(cfg) = config.consensus.clone() {
        let pool = connection_pool.clone();
        let mut stop_receiver = stop_receiver.clone();
        let sync_state = sync_state.clone();
        let main_node_client = main_node_client.clone();

        tokio::spawn(async move {
            scope::run!(&ctx::root(), |ctx, s| async {
                s.spawn_bg(async {
                    let res = cfg.run(ctx, pool, action_queue_sender).await;
                    tracing::info!("Consensus actor stopped");
                    res
                });
                // TODO: information about the head block of the validators (currently just the main node)
                //   should also be provided over the gossip network.
                s.spawn_bg(async {
                    consensus::run_main_node_state_fetcher(ctx, &main_node_client, &sync_state)
                        .await?;
                    Ok(())
                });
                ctx.wait(stop_receiver.wait_for(|stop| *stop)).await??;
                Ok(())
            })
            .await
            .context("consensus actor")
        })
    } else {
        let pool = singleton_pool_builder
            .build()
            .await
            .context("failed to build a connection pool for `MainNodeFetcher`")?;
        let mut storage = pool.access_storage_tagged("sync_layer").await?;
        let fetcher = MainNodeFetcher::new(
            &mut storage,
            Box::new(main_node_client.clone()),
            action_queue_sender,
            sync_state.clone(),
            stop_receiver.clone(),
        )
        .await
        .context("failed initializing main node fetcher")?;
        tokio::spawn(fetcher.run())
    };

    let metadata_calculator_config = MetadataCalculatorConfig {
        db_path: config.required.merkle_tree_path.clone(),
        mode: MerkleTreeMode::Lightweight,
        delay_interval: config.optional.metadata_calculator_delay(),
        max_l1_batches_per_iter: config.optional.max_l1_batches_per_tree_iter,
        multi_get_chunk_size: config.optional.merkle_tree_multi_get_chunk_size,
        block_cache_capacity: config.optional.merkle_tree_block_cache_size(),
        memtable_capacity: config.optional.merkle_tree_memtable_capacity(),
        stalled_writes_timeout: config.optional.merkle_tree_stalled_writes_timeout(),
    };
    let metadata_calculator = MetadataCalculator::new(metadata_calculator_config, None)
        .await
        .context("failed initializing metadata calculator")?;
    app_health.insert_component(metadata_calculator.tree_health_check());

    let kzg_settings = Some(Arc::new(KzgSettings::new(
        &config.optional.kzg_trusted_setup_path,
    )));
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
        kzg_settings,
    );
    app_health.insert_component(consistency_checker.health_check().clone());
    let consistency_checker_handle = tokio::spawn(consistency_checker.run(stop_receiver.clone()));

    let batch_status_updater = BatchStatusUpdater::new(
        main_node_client.clone(),
        singleton_pool_builder
            .build()
            .await
            .context("failed to build a connection pool for BatchStatusUpdater")?,
    );
    app_health.insert_component(batch_status_updater.health_check());

    // Run the components.
    let tree_stop_receiver = stop_receiver.clone();
    let tree_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a tree_pool")?;
    let tree_handle = task::spawn(metadata_calculator.run(tree_pool, tree_stop_receiver));

    let commitment_generator_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a commitment_generator_pool")?;
    let commitment_generator = CommitmentGenerator::new(
        commitment_generator_pool,
        &config.optional.kzg_trusted_setup_path,
    );
    app_health.insert_component(commitment_generator.health_check());
    let commitment_generator_handle = tokio::spawn(commitment_generator.run(stop_receiver.clone()));

    let updater_handle = task::spawn(batch_status_updater.run(stop_receiver.clone()));
    let fee_address_migration_handle =
        task::spawn(state_keeper.run_fee_address_migration(connection_pool.clone()));
    let sk_handle = task::spawn(state_keeper.run());
    let fee_params_fetcher_handle =
        tokio::spawn(fee_params_fetcher.clone().run(stop_receiver.clone()));

    let (tx_sender, vm_barrier, cache_update_handle, proxy_cache_updater_handle) = {
        let tx_proxy = TxProxy::new(main_node_client);
        let proxy_cache_updater_pool = singleton_pool_builder
            .build()
            .await
            .context("failed to build a tree_pool")?;
        let proxy_cache_updater_handle = tokio::spawn(
            tx_proxy
                .run_account_nonce_sweeper(proxy_cache_updater_pool.clone(), stop_receiver.clone()),
        );

        let tx_sender_builder = TxSenderBuilder::new(
            config.clone().into(),
            connection_pool.clone(),
            Arc::new(tx_proxy),
        );

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
            task::spawn_blocking(storage_caches.configure_storage_values_cache(
                latest_values_cache_size,
                connection_pool.clone(),
                tokio::runtime::Handle::current(),
            ))
        });

        let tx_sender = tx_sender_builder
            .build(
                fee_params_fetcher,
                Arc::new(vm_concurrency_limiter),
                ApiContracts::load_from_disk(), // TODO (BFT-138): Allow to dynamically reload API contracts
                storage_caches,
            )
            .await;
        (
            tx_sender,
            vm_barrier,
            cache_update_handle,
            proxy_cache_updater_handle,
        )
    };

    let http_server_handles =
        ApiBuilder::jsonrpsee_backend(config.clone().into(), connection_pool.clone())
            .http(config.required.http_port)
            .with_filter_limit(config.optional.filters_limit)
            .with_batch_request_size_limit(config.optional.max_batch_request_size)
            .with_response_body_size_limit(config.optional.max_response_body_size())
            .with_tx_sender(tx_sender.clone())
            .with_vm_barrier(vm_barrier.clone())
            .with_sync_state(sync_state.clone())
            .enable_api_namespaces(config.optional.api_namespaces())
            .build()
            .context("failed to build HTTP JSON-RPC server")?
            .run(stop_receiver.clone())
            .await
            .context("Failed initializing HTTP JSON-RPC server")?;

    let ws_server_handles =
        ApiBuilder::jsonrpsee_backend(config.clone().into(), connection_pool.clone())
            .ws(config.required.ws_port)
            .with_filter_limit(config.optional.filters_limit)
            .with_subscriptions_limit(config.optional.subscriptions_limit)
            .with_batch_request_size_limit(config.optional.max_batch_request_size)
            .with_response_body_size_limit(config.optional.max_response_body_size())
            .with_polling_interval(config.optional.polling_interval())
            .with_tx_sender(tx_sender)
            .with_vm_barrier(vm_barrier)
            .with_sync_state(sync_state)
            .enable_api_namespaces(config.optional.api_namespaces())
            .build()
            .context("failed to build WS JSON-RPC server")?
            .run(stop_receiver.clone())
            .await
            .context("Failed initializing WS JSON-RPC server")?;

    app_health.insert_component(ws_server_handles.health_check);
    app_health.insert_component(http_server_handles.health_check);

    if let Some(port) = config.optional.prometheus_port {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");
        app_health.insert_component(prometheus_health_check);
        task_handles.push(tokio::spawn(async move {
            prometheus_health_updater.update(HealthStatus::Ready.into());
            let result = PrometheusExporterConfig::pull(port)
                .run(stop_receiver)
                .await;
            drop(prometheus_health_updater);
            result
        }));
    }

    task_handles.extend(http_server_handles.tasks);
    task_handles.extend(ws_server_handles.tasks);
    task_handles.extend(cache_update_handle);
    task_handles.push(proxy_cache_updater_handle);
    task_handles.extend([
        sk_handle,
        fee_address_migration_handle,
        fetcher_handle,
        updater_handle,
        tree_handle,
        consistency_checker_handle,
        fee_params_fetcher_handle,
        commitment_generator_handle,
    ]);

    Ok(())
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
    /// Enables application-level snapshot recovery. Required to start a node that was recovered from a snapshot,
    /// or to initialize a node from a snapshot. Has no effect if a node that was initialized from a Postgres dump
    /// or was synced from genesis.
    ///
    /// This is an experimental and incomplete feature; do not use unless you know what you're doing.
    #[arg(long, conflicts_with = "enable_consensus")]
    enable_snapshots_recovery: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initial setup.
    let opt = Cli::parse();

    let observability_config =
        observability_config_from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &observability_config.sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let mut config = ExternalNodeConfig::collect()
        .await
        .context("Failed to load external node config")?;
    if opt.enable_consensus {
        // This is more of a sanity check; the mutual exclusion of `enable_consensus` and `enable_snapshots_recovery`
        // should be ensured by `clap`.
        anyhow::ensure!(
            !opt.enable_snapshots_recovery,
            "Consensus logic does not support snapshot recovery yet"
        );
        config.consensus =
            Some(config::read_consensus_config().context("read_consensus_config()")?);
    }

    if let Some(threshold) = config.optional.slow_query_threshold() {
        ConnectionPool::global_config().set_slow_query_threshold(threshold)?;
    }
    if let Some(threshold) = config.optional.long_connection_threshold() {
        ConnectionPool::global_config().set_long_connection_threshold(threshold)?;
    }

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

        let mut connection = connection_pool.access_storage().await?;
        let sealed_l1_batch_number = connection
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("Failed getting sealed L1 batch number")?
            .context(
                "Cannot roll back pending L1 batch since there are no L1 batches in Postgres",
            )?;
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
    let main_node_url = config
        .required
        .main_node_url()
        .expect("Main node URL is incorrect");
    tracing::info!("Main node URL is: {main_node_url}");

    let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
        .context("Failed creating JSON-RPC client for main node")?;
    let app_health = Arc::new(AppHealthCheck::new(
        config.optional.healthcheck_slow_time_limit(),
        config.optional.healthcheck_hard_time_limit(),
    ));
    app_health.insert_custom_component(Arc::new(MainNodeHealthCheck::from(
        main_node_client.clone(),
    )));
    app_health.insert_custom_component(Arc::new(ConnectionPoolHealthCheck::new(
        connection_pool.clone(),
    )));

    // Start the health check server early into the node lifecycle so that its health can be monitored from the very start.
    let healthcheck_handle = HealthCheckHandle::spawn_server(
        ([0, 0, 0, 0], config.required.healthcheck_port).into(),
        app_health.clone(),
    );

    // Make sure that the node storage is initialized either via genesis or snapshot recovery.
    ensure_storage_initialized(
        &connection_pool,
        &main_node_client,
        &app_health,
        config.remote.l2_chain_id,
        opt.enable_snapshots_recovery,
    )
    .await?;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut task_handles = vec![];
    init_tasks(
        &config,
        connection_pool.clone(),
        main_node_client.clone(),
        &mut task_handles,
        &app_health,
        stop_receiver.clone(),
    )
    .await
    .context("init_tasks")?;

    let reorg_detector = ReorgDetector::new(main_node_client, connection_pool.clone());
    app_health.insert_component(reorg_detector.health_check().clone());
    let mut reorg_detector_handle = tokio::spawn(reorg_detector.run(stop_receiver)).fuse();
    let mut reorg_detector_result = None;

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;

    tokio::select! {
        _ = wait_for_tasks(task_handles, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = sigint_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
        result = &mut reorg_detector_handle => {
            tracing::info!("Reorg detector terminated, shutting down");
            reorg_detector_result = Some(result);
        }
    };

    // Reaching this point means that either some actor exited unexpectedly or we received a stop signal.
    // Broadcast the stop signal to all actors and exit.
    shutdown_components(stop_sender, healthcheck_handle).await;

    if !reorg_detector_handle.is_terminated() {
        reorg_detector_result = Some(reorg_detector_handle.await);
    }
    let reorg_detector_last_correct_batch = reorg_detector_result.and_then(|result| match result {
        Ok(Ok(last_correct_batch)) => last_correct_batch,
        Ok(Err(err)) => {
            tracing::error!("Reorg detector failed: {err:#}");
            None
        }
        Err(err) => {
            tracing::error!("Reorg detector panicked: {err}");
            None
        }
    });

    if let Some(last_correct_batch) = reorg_detector_last_correct_batch {
        tracing::info!("Performing rollback to L1 batch #{last_correct_batch}");

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
