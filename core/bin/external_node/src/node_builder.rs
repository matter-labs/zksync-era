//! This module provides a "builder" for the external node,
//! as well as an interface to run the node with the specified components.

use anyhow::Context as _;
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig},
        database::MerkleTreeMode,
        DatabaseSecrets,
    },
    PostgresConfig,
};
use zksync_metadata_calculator::{MetadataCalculatorConfig, MetadataCalculatorRecoveryConfig};
use zksync_node_api_server::{tx_sender::ApiContracts, web3::Namespace};
use zksync_node_framework::{
    implementations::layers::{
        batch_status_updater::BatchStatusUpdaterLayer,
        commitment_generator::CommitmentGeneratorLayer,
        consensus::{ConsensusLayer, Mode},
        consistency_checker::ConsistencyCheckerLayer,
        healtcheck_server::HealthCheckLayer,
        l1_batch_commitment_mode_validation::L1BatchCommitmentModeValidationLayer,
        main_node_client::MainNodeClientLayer,
        main_node_fee_params_fetcher::MainNodeFeeParamsFetcherLayer,
        metadata_calculator::MetadataCalculatorLayer,
        pools_layer::PoolsLayerBuilder,
        postgres_metrics::PostgresMetricsLayer,
        prometheus_exporter::PrometheusExporterLayer,
        pruning::PruningLayer,
        query_eth_client::QueryEthClientLayer,
        sigint::SigintHandlerLayer,
        state_keeper::{
            external_io::ExternalIOLayer, main_batch_executor::MainBatchExecutorLayer,
            output_handler::OutputHandlerLayer, StateKeeperLayer,
        },
        sync_state_updater::SyncStateUpdaterLayer,
        tree_data_fetcher::TreeDataFetcherLayer,
        validate_chain_ids::ValidateChainIdsLayer,
        web3_api::{
            caches::MempoolCacheLayer,
            server::{Web3ServerLayer, Web3ServerOptionalConfig},
            tree_api_client::TreeApiClientLayer,
            tx_sender::{PostgresStorageCachesConfig, TxSenderLayer},
            tx_sink::TxSinkLayer,
        },
    },
    service::{ZkStackService, ZkStackServiceBuilder},
};
use zksync_state::RocksdbStorageOptions;

use crate::{
    config::{self, ExternalNodeConfig},
    Component,
};

/// Builder for the external node.
#[derive(Debug)]
pub(crate) struct ExternalNodeBuilder {
    node: ZkStackServiceBuilder,
    config: ExternalNodeConfig,
}

impl ExternalNodeBuilder {
    pub fn new(config: ExternalNodeConfig) -> Self {
        Self {
            node: ZkStackServiceBuilder::new(),
            config,
        }
    }

    fn add_sigint_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SigintHandlerLayer);
        Ok(self)
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        // Note: the EN config doesn't currently support specifying configuration for replicas,
        // so we reuse the master configuration for that purpose.
        // Settings unconditionally set to `None` are either not supported by the EN configuration layer
        // or are not used in the context of the external node.
        let config = PostgresConfig {
            max_connections: Some(self.config.postgres.max_connections),
            max_connections_master: Some(self.config.postgres.max_connections),
            acquire_timeout_sec: None,
            statement_timeout_sec: None,
            long_connection_threshold_ms: None,
            slow_query_threshold_ms: self
                .config
                .optional
                .slow_query_threshold()
                .map(|d| d.as_millis() as u64),
            test_server_url: None,
            test_prover_url: None,
        };
        let secrets = DatabaseSecrets {
            server_url: Some(self.config.postgres.database_url()),
            server_replica_url: Some(self.config.postgres.database_url()),
            prover_url: None,
        };
        let pools_layer = PoolsLayerBuilder::empty(config, secrets)
            .with_master(true)
            .with_replica(true)
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_postgres_metrics_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(PostgresMetricsLayer);
        Ok(self)
    }

    fn add_main_node_client_layer(mut self) -> anyhow::Result<Self> {
        let layer = MainNodeClientLayer::new(
            self.config.required.main_node_url.clone(),
            self.config.optional.main_node_rate_limit_rps,
            self.config.required.l2_chain_id,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_healthcheck_layer(mut self) -> anyhow::Result<Self> {
        let healthcheck_config = HealthCheckConfig {
            port: self.config.required.healthcheck_port,
            slow_time_limit_ms: self
                .config
                .optional
                .healthcheck_slow_time_limit()
                .map(|d| d.as_millis() as u64),
            hard_time_limit_ms: self
                .config
                .optional
                .healthcheck_hard_time_limit()
                .map(|d| d.as_millis() as u64),
        };
        self.node.add_layer(HealthCheckLayer(healthcheck_config));
        Ok(self)
    }

    fn add_prometheus_exporter_layer(mut self) -> anyhow::Result<Self> {
        if let Some(prom_config) = self.config.observability.prometheus() {
            self.node.add_layer(PrometheusExporterLayer(prom_config));
        } else {
            tracing::info!("No configuration for prometheus exporter, skipping");
        }
        Ok(self)
    }

    fn add_query_eth_client_layer(mut self) -> anyhow::Result<Self> {
        let query_eth_client_layer = QueryEthClientLayer::new(
            self.config.required.l1_chain_id,
            self.config.required.eth_client_url.clone(),
        );
        self.node.add_layer(query_eth_client_layer);
        Ok(self)
    }

    fn add_state_keeper_layer(mut self) -> anyhow::Result<Self> {
        // While optional bytecode compression may be disabled on the main node, there are batches where
        // optional bytecode compression was enabled. To process these batches (and also for the case where
        // compression will become optional on the sequencer again), EN has to allow txs without bytecode
        // compression.
        const OPTIONAL_BYTECODE_COMPRESSION: bool = true;

        let persistence_layer = OutputHandlerLayer::new(
            self.config
                .remote
                .l2_shared_bridge_addr
                .expect("L2 shared bridge address is not set"),
            self.config.optional.l2_block_seal_queue_capacity,
        )
        .with_pre_insert_txs(true) // EN requires txs to be pre-inserted.
        .with_protective_reads_persistence_enabled(
            self.config.optional.protective_reads_persistence_enabled,
        );

        let io_layer = ExternalIOLayer::new(self.config.required.l2_chain_id);

        // We only need call traces on the external node if the `debug_` namespace is enabled.
        let save_call_traces = self
            .config
            .optional
            .api_namespaces()
            .contains(&Namespace::Debug);
        let main_node_batch_executor_builder_layer =
            MainBatchExecutorLayer::new(save_call_traces, OPTIONAL_BYTECODE_COMPRESSION);

        let rocksdb_options = RocksdbStorageOptions {
            block_cache_capacity: self
                .config
                .experimental
                .state_keeper_db_block_cache_capacity(),
            max_open_files: self.config.experimental.state_keeper_db_max_open_files,
        };
        let state_keeper_layer = StateKeeperLayer::new(
            self.config.required.state_cache_path.clone(),
            rocksdb_options,
        );
        self.node
            .add_layer(persistence_layer)
            .add_layer(io_layer)
            .add_layer(main_node_batch_executor_builder_layer)
            .add_layer(state_keeper_layer);
        Ok(self)
    }

    fn add_consensus_layer(mut self) -> anyhow::Result<Self> {
        let config = self.config.consensus.clone();
        let secrets =
            config::read_consensus_secrets().context("config::read_consensus_secrets()")?;
        let layer = ConsensusLayer {
            mode: Mode::External,
            config,
            secrets,
        };
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_pruning_layer(mut self) -> anyhow::Result<Self> {
        if self.config.optional.pruning_enabled {
            let layer = PruningLayer::new(
                self.config.optional.pruning_removal_delay(),
                self.config.optional.pruning_chunk_size,
                self.config.optional.pruning_data_retention(),
            );
            self.node.add_layer(layer);
        } else {
            tracing::info!("Pruning is disabled");
        }
        Ok(self)
    }

    fn add_l1_batch_commitment_mode_validation_layer(mut self) -> anyhow::Result<Self> {
        let layer = L1BatchCommitmentModeValidationLayer::new(
            self.config.remote.diamond_proxy_addr,
            self.config.optional.l1_batch_commit_data_generator_mode,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_validate_chain_ids_layer(mut self) -> anyhow::Result<Self> {
        let layer = ValidateChainIdsLayer::new(
            self.config.required.l1_chain_id,
            self.config.required.l2_chain_id,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_consistency_checker_layer(mut self) -> anyhow::Result<Self> {
        let max_batches_to_recheck = 10; // TODO (BFT-97): Make it a part of a proper EN config
        let layer = ConsistencyCheckerLayer::new(
            self.config.remote.diamond_proxy_addr,
            max_batches_to_recheck,
            self.config.optional.l1_batch_commit_data_generator_mode,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_commitment_generator_layer(mut self) -> anyhow::Result<Self> {
        let layer =
            CommitmentGeneratorLayer::new(self.config.optional.l1_batch_commit_data_generator_mode)
                .with_max_parallelism(
                    self.config
                        .experimental
                        .commitment_generator_max_parallelism,
                );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_batch_status_updater_layer(mut self) -> anyhow::Result<Self> {
        let layer = BatchStatusUpdaterLayer;
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_tree_data_fetcher_layer(mut self) -> anyhow::Result<Self> {
        let layer = TreeDataFetcherLayer::new(self.config.remote.diamond_proxy_addr);
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_sync_state_updater_layer(mut self) -> anyhow::Result<Self> {
        // This layer may be used as a fallback for EN API if API server runs without the core component.
        self.node.add_layer(SyncStateUpdaterLayer);
        Ok(self)
    }

    fn add_metadata_calculator_layer(mut self, with_tree_api: bool) -> anyhow::Result<Self> {
        let metadata_calculator_config = MetadataCalculatorConfig {
            db_path: self.config.required.merkle_tree_path.clone(),
            max_open_files: self.config.optional.merkle_tree_max_open_files,
            mode: MerkleTreeMode::Lightweight,
            delay_interval: self.config.optional.merkle_tree_processing_delay(),
            max_l1_batches_per_iter: self.config.optional.merkle_tree_max_l1_batches_per_iter,
            multi_get_chunk_size: self.config.optional.merkle_tree_multi_get_chunk_size,
            block_cache_capacity: self.config.optional.merkle_tree_block_cache_size(),
            include_indices_and_filters_in_block_cache: self
                .config
                .optional
                .merkle_tree_include_indices_and_filters_in_block_cache,
            memtable_capacity: self.config.optional.merkle_tree_memtable_capacity(),
            stalled_writes_timeout: self.config.optional.merkle_tree_stalled_writes_timeout(),
            recovery: MetadataCalculatorRecoveryConfig {
                desired_chunk_size: self.config.experimental.snapshots_recovery_tree_chunk_size,
                parallel_persistence_buffer: self
                    .config
                    .experimental
                    .snapshots_recovery_tree_parallel_persistence_buffer,
            },
        };

        // Configure basic tree layer.
        let mut layer = MetadataCalculatorLayer::new(metadata_calculator_config);

        // Add tree API if needed.
        if with_tree_api {
            let merkle_tree_api_config = MerkleTreeApiConfig {
                port: self
                    .config
                    .tree_component
                    .api_port
                    .context("should contain tree api port")?,
            };
            layer = layer.with_tree_api_config(merkle_tree_api_config);
        }

        // Add tree pruning if needed.
        if self.config.optional.pruning_enabled {
            layer = layer.with_pruning_config(self.config.optional.pruning_removal_delay());
        }

        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_tx_sender_layer(mut self) -> anyhow::Result<Self> {
        let postgres_storage_config = PostgresStorageCachesConfig {
            factory_deps_cache_size: self.config.optional.factory_deps_cache_size() as u64,
            initial_writes_cache_size: self.config.optional.initial_writes_cache_size() as u64,
            latest_values_cache_size: self.config.optional.latest_values_cache_size() as u64,
        };
        let max_vm_concurrency = self.config.optional.vm_concurrency_limit;
        let api_contracts = ApiContracts::load_from_disk_blocking(); // TODO (BFT-138): Allow to dynamically reload API contracts;
        let tx_sender_layer = TxSenderLayer::new(
            (&self.config).into(),
            postgres_storage_config,
            max_vm_concurrency,
            api_contracts,
        )
        .with_whitelisted_tokens_for_aa_cache(true);

        self.node.add_layer(TxSinkLayer::ProxySink);
        self.node.add_layer(tx_sender_layer);
        Ok(self)
    }

    fn add_mempool_cache_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(MempoolCacheLayer::new(
            self.config.optional.mempool_cache_size,
            self.config.optional.mempool_cache_update_interval(),
        ));
        Ok(self)
    }

    fn add_tree_api_client_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(TreeApiClientLayer::http(
            self.config.api_component.tree_api_remote_url.clone(),
        ));
        Ok(self)
    }

    fn add_main_node_fee_params_fetcher_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(MainNodeFeeParamsFetcherLayer);
        Ok(self)
    }

    fn web3_api_optional_config(&self) -> Web3ServerOptionalConfig {
        // The refresh interval should be several times lower than the pruning removal delay, so that
        // soft-pruning will timely propagate to the API server.
        let pruning_info_refresh_interval = self.config.optional.pruning_removal_delay() / 5;

        Web3ServerOptionalConfig {
            namespaces: Some(self.config.optional.api_namespaces()),
            filters_limit: Some(self.config.optional.filters_limit),
            subscriptions_limit: Some(self.config.optional.filters_limit),
            batch_request_size_limit: Some(self.config.optional.max_batch_request_size),
            response_body_size_limit: Some(self.config.optional.max_response_body_size()),
            with_extended_tracing: self.config.optional.extended_rpc_tracing,
            pruning_info_refresh_interval: Some(pruning_info_refresh_interval),
            websocket_requests_per_minute_limit: None, // To be set by WS server layer method if required.
            replication_lag_limit: None,               // TODO: Support replication lag limit
        }
    }

    fn add_http_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let optional_config = self.web3_api_optional_config();
        self.node.add_layer(Web3ServerLayer::http(
            self.config.required.http_port,
            (&self.config).into(),
            optional_config,
        ));

        Ok(self)
    }

    fn add_ws_web3_api_layer(mut self) -> anyhow::Result<Self> {
        // TODO: Support websocket requests per minute limit
        let optional_config = self.web3_api_optional_config();
        self.node.add_layer(Web3ServerLayer::ws(
            self.config.required.ws_port,
            (&self.config).into(),
            optional_config,
        ));

        Ok(self)
    }

    pub fn build(mut self, mut components: Vec<Component>) -> anyhow::Result<ZkStackService> {
        // Add "base" layers
        self = self
            .add_sigint_handler_layer()?
            .add_healthcheck_layer()?
            .add_prometheus_exporter_layer()?
            .add_pools_layer()?
            .add_main_node_client_layer()?
            .add_query_eth_client_layer()?;

        // Add preconditions for all the components.
        self = self
            .add_l1_batch_commitment_mode_validation_layer()?
            .add_validate_chain_ids_layer()?;

        // Sort the components, so that the components they may depend on each other are added in the correct order.
        components.sort_unstable_by_key(|component| match component {
            // API consumes the resources provided by other layers (multiple ones), so it has to come the last.
            Component::HttpApi | Component::WsApi => 1,
            // Default priority.
            _ => 0,
        });

        for component in &components {
            match component {
                Component::HttpApi => {
                    self = self
                        .add_sync_state_updater_layer()?
                        .add_mempool_cache_layer()?
                        .add_tree_api_client_layer()?
                        .add_main_node_fee_params_fetcher_layer()?
                        .add_tx_sender_layer()?
                        .add_http_web3_api_layer()?;
                }
                Component::WsApi => {
                    self = self
                        .add_sync_state_updater_layer()?
                        .add_mempool_cache_layer()?
                        .add_tree_api_client_layer()?
                        .add_main_node_fee_params_fetcher_layer()?
                        .add_tx_sender_layer()?
                        .add_ws_web3_api_layer()?;
                }
                Component::Tree => {
                    // Right now, distributed mode for EN is not fully supported, e.g. there are some
                    // issues with reorg detection and snapshot recovery.
                    // So we require the core component to be present, e.g. forcing the EN to run in a monolithic mode.
                    anyhow::ensure!(
                        components.contains(&Component::Core),
                        "Tree must run on the same machine as Core"
                    );
                    let with_tree_api = components.contains(&Component::TreeApi);
                    self = self.add_metadata_calculator_layer(with_tree_api)?;
                }
                Component::TreeApi => {
                    anyhow::ensure!(
                        components.contains(&Component::Tree),
                        "Merkle tree API cannot be started without a tree component"
                    );
                    // Do nothing, will be handled by the `Tree` component.
                }
                Component::TreeFetcher => {
                    self = self.add_tree_data_fetcher_layer()?;
                }
                Component::Core => {
                    // Core is a singleton & mandatory component,
                    // so until we have a dedicated component for "auxiliary" tasks,
                    // it's responsible for things like metrics.
                    self = self.add_postgres_metrics_layer()?;

                    // Main tasks
                    self = self
                        .add_state_keeper_layer()?
                        .add_consensus_layer()?
                        .add_pruning_layer()?
                        .add_consistency_checker_layer()?
                        .add_commitment_generator_layer()?
                        .add_batch_status_updater_layer()?;
                }
            }
        }

        Ok(self.node.build()?)
    }
}
