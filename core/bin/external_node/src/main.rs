use std::{future, sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use metrics::EN_METRICS;
use prometheus_exporter::PrometheusExporterConfig;
use tokio::{sync::watch, task, time::sleep};
use zksync_basic_types::{Address, L2ChainId};
use zksync_concurrency::{ctx, limiter, scope, time};
use zksync_config::configs::database::MerkleTreeMode;
use zksync_core::{
    api_server::{
        execution_sandbox::VmConcurrencyLimiter,
        healthcheck::HealthCheckHandle,
        tx_sender::{proxy::TxProxy, ApiContracts, TxSenderBuilder},
        web3::{ApiBuilder, Namespace},
    },
    block_reverter::{BlockReverter, BlockReverterFlags, L1ExecutedBatchesRevert, NodeRole},
    commitment_generator::CommitmentGenerator,
    consensus,
    consistency_checker::ConsistencyChecker,
    db_pruner::{
        prune_conditions::{
            L1BatchExistsCondition, L1BatchOlderThanPruneCondition,
            NextL1BatchHasMetadataCondition, NextL1BatchWasExecutedCondition,
        },
        DbPruner, DbPrunerConfig,
    },
    l1_gas_price::MainNodeFeeParamsFetcher,
    metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig},
    reorg_detector,
    reorg_detector::ReorgDetector,
    setup_sigint_handler,
    state_keeper::{
        seal_criteria::NoopSealer, BatchExecutor, MainBatchExecutor, MiniblockSealer,
        MiniblockSealerHandle, ZkSyncStateKeeper,
    },
    sync_layer::{
        batch_status_updater::BatchStatusUpdater, external_io::ExternalIO, ActionQueue,
        MainNodeClient, SyncState,
    },
};
use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core, CoreDal};
use zksync_db_connection::healthcheck::ConnectionPoolHealthCheck;
use zksync_health_check::{AppHealthCheck, HealthStatus, ReactiveHealthCheck};
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
mod version_sync_task;

const RELEASE_MANIFEST: &str = include_str!("../../../../.github/release-please/manifest.json");

/// Creates the state keeper configured to work in the external node mode.
#[allow(clippy::too_many_arguments)]
async fn build_state_keeper(
    action_queue: ActionQueue,
    state_keeper_db_path: String,
    config: &ExternalNodeConfig,
    connection_pool: ConnectionPool<Core>,
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
    connection_pool: ConnectionPool<Core>,
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
                .connection()
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

    task_handles.push(tokio::spawn({
        let ctx = ctx::root();
        let cfg = config.consensus.clone();
        let mut stop_receiver = stop_receiver.clone();
        let fetcher = consensus::Fetcher {
            store: consensus::Store(connection_pool.clone()),
            sync_state: sync_state.clone(),
            client: Box::new(main_node_client.clone()),
            limiter: limiter::Limiter::new(
                &ctx,
                limiter::Rate {
                    burst: 10,
                    refresh: time::Duration::milliseconds(30),
                },
            ),
        };
        let actions = action_queue_sender;
        async move {
            scope::run!(&ctx, |ctx, s| async {
                s.spawn_bg(async {
                    let res = match cfg {
                        Some(cfg) => {
                            let secrets = config::read_consensus_secrets()
                                .context("config::read_consensus_secrets()")?
                                .context("consensus secrets missing")?;
                            fetcher.run_p2p(ctx, actions, cfg.p2p(&secrets)?).await
                        }
                        None => fetcher.run_centralized(ctx, actions).await,
                    };
                    tracing::info!("Consensus actor stopped");
                    res
                });
                ctx.wait(stop_receiver.wait_for(|stop| *stop)).await??;
                Ok(())
            })
            .await
            .context("consensus actor")
        }
    }));

    let pruning_enabled = config.optional.pruning_data_retention_hours.is_some();
    if pruning_enabled {
        let l1_batch_age_to_prune =
            Duration::from_secs(3600 * config.optional.pruning_data_retention_hours.unwrap());
        tracing::info!(
            "Configured pruning of batches after they become {l1_batch_age_to_prune:?} old"
        );
        let db_pruner = DbPruner::new(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::from_secs(60),
                pruned_chunk_size: config.optional.pruning_chunk_size,
                next_iterations_delay: Duration::from_secs(30),
            },
            vec![
                Arc::new(L1BatchExistsCondition {
                    conn: connection_pool.clone(),
                }),
                Arc::new(NextL1BatchHasMetadataCondition {
                    conn: connection_pool.clone(),
                }),
                Arc::new(NextL1BatchWasExecutedCondition {
                    conn: connection_pool.clone(),
                }),
                Arc::new(L1BatchOlderThanPruneCondition {
                    minimal_age: l1_batch_age_to_prune,
                    conn: connection_pool.clone(),
                }),
            ],
        )?;
        task_handles.push(tokio::spawn(
            db_pruner.run_in_loop(connection_pool.clone(), stop_receiver.clone()),
        ));
    }

    let reorg_detector = ReorgDetector::new(main_node_client.clone(), connection_pool.clone());
    app_health.insert_component(reorg_detector.health_check().clone());
    task_handles.push(tokio::spawn({
        let stop = stop_receiver.clone();
        async move {
            reorg_detector
                .run(stop)
                .await
                .context("reorg_detector.run()")
        }
    }));

    let singleton_pool_builder = ConnectionPool::<Core>::singleton(&config.postgres.database_url);

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

    let remote_diamond_proxy_addr = config.remote.diamond_proxy_addr;
    let diamond_proxy_addr = if let Some(addr) = config.optional.contracts_diamond_proxy_addr {
        anyhow::ensure!(
            addr == remote_diamond_proxy_addr,
            "Diamond proxy address {addr:?} specified in config doesn't match one returned by main node \
             ({remote_diamond_proxy_addr:?})"
        );
        addr
    } else {
        tracing::info!(
            "Diamond proxy address is not specified in config; will use address returned by main node: {remote_diamond_proxy_addr:?}"
        );
        remote_diamond_proxy_addr
    };

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
    )
    .context("cannot initialize consistency checker")?
    .with_diamond_proxy_addr(diamond_proxy_addr);

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
    let tree_reader = Arc::new(metadata_calculator.tree_reader());
    let tree_handle = task::spawn(metadata_calculator.run(tree_pool, tree_stop_receiver));

    let commitment_generator_pool = singleton_pool_builder
        .build()
        .await
        .context("failed to build a commitment_generator_pool")?;
    let commitment_generator = CommitmentGenerator::new(commitment_generator_pool);
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
            task::spawn(
                storage_caches
                    .configure_storage_values_cache(
                        latest_values_cache_size,
                        connection_pool.clone(),
                    )
                    .run(stop_receiver.clone()),
            )
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
            .with_tree_api(tree_reader.clone())
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
            .with_tree_api(tree_reader)
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
    if !opt.enable_consensus {
        config.consensus = None;
    }
    if let Some(threshold) = config.optional.slow_query_threshold() {
        ConnectionPool::<Core>::global_config().set_slow_query_threshold(threshold)?;
    }
    if let Some(threshold) = config.optional.long_connection_threshold() {
        ConnectionPool::<Core>::global_config().set_long_connection_threshold(threshold)?;
    }

    let connection_pool = ConnectionPool::<Core>::builder(
        &config.postgres.database_url,
        config.postgres.max_connections,
    )
    .build()
    .await
    .context("failed to build a connection_pool")?;

    let main_node_url = config
        .required
        .main_node_url()
        .expect("Main node URL is incorrect");
    tracing::info!("Main node URL is: {main_node_url}");
    let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
        .context("Failed creating JSON-RPC client for main node")?;

    let sigint_receiver = setup_sigint_handler();
    tracing::warn!("The external node is in the alpha phase, and should be used with caution.");
    tracing::info!("Started the external node");

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
    // Start scraping Postgres metrics before store initialization as well.
    let metrics_pool = connection_pool.clone();
    let version_sync_task_pool = connection_pool.clone();
    let version_sync_task_main_node_client = main_node_client.clone();
    let mut task_handles = vec![
        tokio::spawn(async move {
            PostgresMetrics::run_scraping(metrics_pool, Duration::from_secs(60)).await;
            Ok(())
        }),
        tokio::spawn(async move {
            version_sync_task::sync_versions(
                version_sync_task_pool,
                version_sync_task_main_node_client,
            )
            .await?;
            future::pending::<()>().await;
            // ^ Since this is run as a task, we don't want it to exit on success (this would shut down the node).
            Ok(())
        }),
    ];

    // Make sure that the node storage is initialized either via genesis or snapshot recovery.
    ensure_storage_initialized(
        &connection_pool,
        &main_node_client,
        &app_health,
        config.remote.l2_chain_id,
        config.optional.snapshots_recovery_enabled,
    )
    .await?;

    // Revert the storage if needed.
    let reverter = BlockReverter::new(
        NodeRole::External,
        config.required.state_cache_path.clone(),
        config.required.merkle_tree_path.clone(),
        None,
        connection_pool.clone(),
        L1ExecutedBatchesRevert::Allowed,
    );

    let mut reorg_detector = ReorgDetector::new(main_node_client.clone(), connection_pool.clone());
    // We're checking for the reorg in the beginning because we expect that if reorg is detected during
    // the node lifecycle, the node will exit the same way as it does with any other critical error,
    // and would restart. Then, on the 2nd launch reorg would be detected here, then processed and the node
    // will be able to operate normally afterwards.
    match reorg_detector.check_consistency().await {
        Ok(()) => {}
        Err(reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
            tracing::info!("Rolling back to l1 batch number {last_correct_l1_batch}");
            reverter
                .rollback_db(last_correct_l1_batch, BlockReverterFlags::all())
                .await;
            tracing::info!("Rollback successfully completed");
        }
        Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
    }
    if opt.revert_pending_l1_batch {
        tracing::info!("Rolling pending L1 batch back..");
        let mut connection = connection_pool.connection().await?;
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
        tracing::info!("Rollback successfully completed");
    }

    let (stop_sender, stop_receiver) = watch::channel(false);
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

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;

    tokio::select! {
        _ = wait_for_tasks(task_handles, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = sigint_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
    };

    // Reaching this point means that either some actor exited unexpectedly or we received a stop signal.
    // Broadcast the stop signal to all actors and exit.
    shutdown_components(stop_sender, healthcheck_handle).await;
    tracing::info!("Stopped");
    Ok(())
}
