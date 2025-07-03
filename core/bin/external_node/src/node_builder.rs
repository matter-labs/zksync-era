//! This module provides a "builder" for the external node,
//! as well as an interface to run the node with the specified components.

use std::mem;

use anyhow::{bail, Context as _};
use zksync_block_reverter::{
    node::{BlockReverterLayer, UnconditionalRevertLayer},
    NodeRole,
};
use zksync_commitment_generator::node::CommitmentGeneratorLayer;
use zksync_config::{
    configs::{
        api::{MerkleTreeApiConfig, Namespace},
        database::MerkleTreeMode,
        DataAvailabilitySecrets,
    },
    DAClientConfig,
};
use zksync_consistency_checker::node::ConsistencyCheckerLayer;
use zksync_da_clients::node::{
    AvailWiringLayer, CelestiaWiringLayer, EigenWiringLayer, NoDAClientWiringLayer,
    ObjectStorageClientWiringLayer,
};
use zksync_dal::node::{PoolsLayer, PostgresMetricsLayer};
use zksync_eth_client::node::BridgeAddressesUpdaterLayer;
use zksync_gateway_migrator::node::SettlementLayerData;
use zksync_logs_bloom_backfill::node::LogsBloomBackfillLayer;
use zksync_metadata_calculator::{
    node::{MetadataCalculatorLayer, TreeApiClientLayer, TreeApiServerLayer},
    MerkleTreeReaderConfig, MetadataCalculatorConfig,
};
use zksync_node_api_server::{
    node::{
        HealthCheckLayer, MempoolCacheLayer, PostgresStorageCachesConfig, ProxySinkLayer,
        TxSenderLayer, Web3ServerLayer, Web3ServerOptionalConfig,
    },
    web3::state::InternalApiConfigBase,
};
use zksync_node_consensus::node::ExternalNodeConsensusLayer;
use zksync_node_db_pruner::node::PruningLayer;
use zksync_node_fee_model::node::MainNodeFeeParamsFetcherLayer;
use zksync_node_framework::service::{ZkStackService, ZkStackServiceBuilder};
use zksync_node_storage_init::{
    node::{external_node_strategy::ExternalNodeInitStrategyLayer, NodeStorageInitializerLayer},
    SnapshotRecoveryConfig,
};
use zksync_node_sync::node::{
    BatchStatusUpdaterLayer, DataAvailabilityFetcherLayer, ExternalIOLayer, SyncStateUpdaterLayer,
    TreeDataFetcherLayer, ValidateChainIdsLayer,
};
use zksync_reorg_detector::node::ReorgDetectorLayer;
use zksync_state::RocksdbStorageOptions;
use zksync_state_keeper::node::{MainBatchExecutorLayer, OutputHandlerLayer, StateKeeperLayer};
use zksync_types::L1BatchNumber;
use zksync_vlog::node::{PrometheusExporterLayer, SigintHandlerLayer};
use zksync_web3_decl::node::{MainNodeClientLayer, QueryEthClientLayer};

use crate::{
    config::{ExternalNodeConfig, RemoteENConfig},
    metrics::framework::ExternalNodeMetricsLayer,
    Component,
};

/// Builder for the external node.
#[derive(Debug)]
pub(crate) struct ExternalNodeBuilder<R = RemoteENConfig> {
    pub(crate) node: ZkStackServiceBuilder,
    config: ExternalNodeConfig<R>,
}

impl<R> ExternalNodeBuilder<R> {
    #[cfg(test)]
    pub fn new(config: ExternalNodeConfig<R>) -> anyhow::Result<Self> {
        Ok(Self {
            node: ZkStackServiceBuilder::new().context("Cannot create ZkStackServiceBuilder")?,
            config,
        })
    }

    pub fn on_runtime(runtime: tokio::runtime::Runtime, config: ExternalNodeConfig<R>) -> Self {
        Self {
            node: ZkStackServiceBuilder::on_runtime(runtime),
            config,
        }
    }

    fn add_sigint_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SigintHandlerLayer);
        Ok(self)
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        let mut config = self.config.local.postgres.clone();
        // Note: the EN config doesn't currently support specifying configuration for replicas,
        // so we reuse the master configuration for that purpose.
        // Settings unconditionally set to `None` are either not supported by the EN configuration layer
        // or are not used in the context of the external node.
        config.max_connections_master = config.max_connections;

        let mut secrets = self.config.local.secrets.postgres.clone();
        secrets.server_replica_url = secrets.server_url.clone();
        secrets.prover_url = None;

        let pools_layer = PoolsLayer::empty(config, secrets)
            .with_master(true)
            .with_replica(true);
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_postgres_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(PostgresMetricsLayer);
        Ok(self)
    }

    #[cfg(not(target_env = "msvc"))]
    fn add_jemalloc_monitor_layer(mut self) -> anyhow::Result<Self> {
        self.node
            .add_layer(zksync_node_jemalloc::JemallocMonitorLayer);
        Ok(self)
    }

    fn add_external_node_metrics_layer(mut self) -> anyhow::Result<Self> {
        let networks = &self.config.local.networks;
        self.node.add_layer(ExternalNodeMetricsLayer {
            l1_chain_id: networks.l1_chain_id,
            sl_chain_id: networks
                .gateway_chain_id
                .unwrap_or(networks.l1_chain_id.into()),
            l2_chain_id: networks.l2_chain_id,
            postgres_pool_size: self.config.local.postgres.max_connections()?,
        });
        Ok(self)
    }

    fn add_main_node_client_layer(mut self) -> anyhow::Result<Self> {
        let networks = &self.config.local.networks;
        let layer = MainNodeClientLayer::new(
            networks.main_node_url.clone(),
            networks.main_node_rate_limit_rps,
            networks.l2_chain_id,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_healthcheck_layer(mut self) -> anyhow::Result<Self> {
        let config = self.config.local.api.healthcheck.clone();
        let config_params = mem::take(&mut self.config.config_params);
        let layer = HealthCheckLayer::new(config).with_config_params(config_params);
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_prometheus_exporter_layer(mut self) -> anyhow::Result<Self> {
        if let Some(prom_config) = self.config.local.prometheus.to_exporter_config() {
            self.node.add_layer(PrometheusExporterLayer(prom_config));
        } else {
            tracing::info!("No configuration for prometheus exporter, skipping");
        }
        Ok(self)
    }

    fn add_query_eth_client_layer(mut self) -> anyhow::Result<Self> {
        let query_eth_client_layer = QueryEthClientLayer::new(
            self.config.local.networks.l1_chain_id,
            self.config
                .local
                .secrets
                .l1
                .l1_rpc_url
                .clone()
                .context("missing L1 RPC URL")?,
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

        let config = &self.config.local;
        let queue_capacity = config.state_keeper.l2_block_seal_queue_capacity;
        let persistence_layer = OutputHandlerLayer::new(queue_capacity)
            .with_pre_insert_txs(true) // EN requires txs to be pre-inserted.
            .with_protective_reads_persistence_enabled(
                config.state_keeper.protective_reads_persistence_enabled,
            );

        let io_layer = ExternalIOLayer::new(config.networks.l2_chain_id);

        // We only need call traces on the external node if the `debug_` namespace is enabled.
        // TODO(PLA-1153): this is backwards / unobvious. Can readily use `config.state_keeper.save_call_traces` instead.
        let save_call_traces = config
            .api
            .web3_json_rpc
            .api_namespaces
            .contains(&Namespace::Debug);
        let main_node_batch_executor_builder_layer =
            MainBatchExecutorLayer::new(save_call_traces, OPTIONAL_BYTECODE_COMPRESSION);

        let db_config = &config.db.experimental;
        let rocksdb_options = RocksdbStorageOptions {
            block_cache_capacity: db_config.state_keeper_db_block_cache_capacity.0 as usize,
            max_open_files: db_config.state_keeper_db_max_open_files,
        };
        let state_keeper_layer =
            StateKeeperLayer::new(config.db.state_keeper_db_path.clone(), rocksdb_options);
        self.node
            .add_layer(io_layer)
            .add_layer(persistence_layer)
            .add_layer(main_node_batch_executor_builder_layer)
            .add_layer(state_keeper_layer);
        Ok(self)
    }

    fn add_consensus_layer(mut self) -> anyhow::Result<Self> {
        let config = self.config.consensus.clone().ok_or_else(||
            anyhow::anyhow!(
            "ZKsync API synchronization is not supported anymore so consensus config is required. \
            Please follow this instruction to enable p2p synchronization: \
            https://github.com/matter-labs/zksync-era/blob/main/docs/src/guides/external-node/10_decentralization.md"
            )
        );
        let secrets = self.config.local.secrets.consensus.clone();
        let layer = ExternalNodeConsensusLayer {
            build_version: crate::metadata::SERVER_VERSION
                .parse()
                .context("CRATE_VERSION.parse()")?,
            config: config.context("Consensus config is missing")?,
            secrets,
        };
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_pruning_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.pruning;
        if config.enabled {
            let layer = PruningLayer::new(
                config.removal_delay,
                config.chunk_size.get(),
                config.data_retention,
            );
            self.node.add_layer(layer);
        } else {
            tracing::info!("Pruning is disabled");
        }
        Ok(self)
    }

    fn add_validate_chain_ids_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.networks;
        let layer = ValidateChainIdsLayer::new(config.l1_chain_id, config.l2_chain_id);
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_consistency_checker_layer(mut self) -> anyhow::Result<Self> {
        let layer = ConsistencyCheckerLayer::new(
            self.config.local.consistency_checker.max_batches_to_recheck,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_commitment_generator_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.commitment_generator;
        let layer =
            CommitmentGeneratorLayer::default().with_max_parallelism(config.max_parallelism);
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_batch_status_updater_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(BatchStatusUpdaterLayer);
        Ok(self)
    }

    fn add_tree_data_fetcher_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(TreeDataFetcherLayer);
        Ok(self)
    }

    fn add_da_client_layer(mut self) -> anyhow::Result<Self> {
        let da_client_config = self.config.local.da_client.clone();
        let da_client_config = da_client_config.context("DA client config is missing")?;

        if matches!(da_client_config, DAClientConfig::NoDA) {
            self.node.add_layer(NoDAClientWiringLayer);
            return Ok(self);
        }

        if let DAClientConfig::ObjectStore(config) = da_client_config {
            self.node
                .add_layer(ObjectStorageClientWiringLayer::new(config));
            return Ok(self);
        }

        let da_client_secrets = self.config.local.secrets.data_availability.clone();
        let da_client_secrets = da_client_secrets.context("DA client secrets are missing")?;
        match (da_client_config, da_client_secrets) {
            (DAClientConfig::Avail(config), DataAvailabilitySecrets::Avail(secret)) => {
                self.node.add_layer(AvailWiringLayer::new(config, secret));
            }
            (DAClientConfig::Celestia(config), DataAvailabilitySecrets::Celestia(secret)) => {
                self.node
                    .add_layer(CelestiaWiringLayer::new(config, secret));
            }
            (DAClientConfig::Eigen(mut config), DataAvailabilitySecrets::Eigen(secret)) => {
                if config.eigenda_eth_rpc.is_none() {
                    config.eigenda_eth_rpc = self.config.local.secrets.l1.l1_rpc_url.clone();
                }
                self.node.add_layer(EigenWiringLayer::new(config, secret));
            }
            _ => bail!("invalid pair of da_client and da_secrets"),
        }

        Ok(self)
    }

    fn add_data_availability_fetcher_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(DataAvailabilityFetcherLayer);
        Ok(self)
    }

    fn add_sync_state_updater_layer(mut self) -> anyhow::Result<Self> {
        // This layer may be used as a fallback for EN API if API server runs without the core component.
        self.node.add_layer(SyncStateUpdaterLayer);
        Ok(self)
    }

    fn add_metadata_calculator_layer(mut self, with_tree_api: bool) -> anyhow::Result<Self> {
        let mut config = self.config.local.db.merkle_tree.clone();
        config.mode = MerkleTreeMode::Lightweight; // Force-override the tree mode; full tree isn't supported on EN yet

        let state_keeper = &self.config.local.state_keeper;
        let snapshot_recovery = &self.config.local.snapshot_recovery;
        let metadata_calculator_config =
            MetadataCalculatorConfig::from_configs(&config, state_keeper, &snapshot_recovery.tree);

        // Configure basic tree layer.
        let mut layer = MetadataCalculatorLayer::new(metadata_calculator_config);

        // Add tree API if needed.
        if with_tree_api {
            let merkle_tree_api_config = MerkleTreeApiConfig {
                port: self.config.local.api.merkle_tree.port,
            };
            layer = layer.with_tree_api_config(merkle_tree_api_config);
        }

        // Add stale keys repair task if requested.
        if self
            .config
            .local
            .db
            .experimental
            .merkle_tree_repair_stale_keys
        {
            layer = layer.with_stale_keys_repair();
        }

        // Add tree pruning if needed.
        let pruning = &self.config.local.pruning;
        if pruning.enabled {
            layer = layer.with_pruning_config(pruning.removal_delay);
        }

        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_isolated_tree_api_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.db.merkle_tree;
        let reader_config = MerkleTreeReaderConfig {
            db_path: config.path.clone(),
            max_open_files: config.max_open_files,
            multi_get_chunk_size: config.multi_get_chunk_size,
            block_cache_capacity: config.block_cache_size.0 as usize,
            include_indices_and_filters_in_block_cache: config
                .include_indices_and_filters_in_block_cache,
        };
        let api_config = MerkleTreeApiConfig {
            port: self.config.local.api.merkle_tree.port,
        };
        self.node
            .add_layer(TreeApiServerLayer::new(reader_config, api_config));
        Ok(self)
    }

    fn add_mempool_cache_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.api.web3_json_rpc;
        self.node.add_layer(MempoolCacheLayer::new(
            config.mempool_cache_size,
            config.mempool_cache_update_interval,
        ));
        Ok(self)
    }

    fn add_tree_api_client_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(TreeApiClientLayer::http(
            self.config.local.api.web3_json_rpc.tree_api_url.clone(),
        ));
        Ok(self)
    }

    fn add_main_node_fee_params_fetcher_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(MainNodeFeeParamsFetcherLayer);
        Ok(self)
    }

    fn add_logs_bloom_backfill_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(LogsBloomBackfillLayer);
        Ok(self)
    }

    fn add_bridge_addresses_updater_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(BridgeAddressesUpdaterLayer {
            refresh_interval: self.config.local.networks.bridge_addresses_refresh_interval,
        });
        Ok(self)
    }

    fn add_reorg_detector_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(ReorgDetectorLayer);
        Ok(self)
    }

    fn add_block_reverter_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.db;
        let mut layer = BlockReverterLayer::new(NodeRole::External);
        // Reverting executed batches is more-or-less safe for external nodes.
        layer
            .allow_rolling_back_executed_batches()
            .enable_rolling_back_postgres()
            .enable_rolling_back_merkle_tree(config.merkle_tree.path.clone())
            .enable_rolling_back_state_keeper_cache(config.state_keeper_db_path.clone());
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_unconditional_revert_layer(mut self, l1_batch: L1BatchNumber) -> anyhow::Result<Self> {
        let layer = UnconditionalRevertLayer::new(l1_batch);
        self.node.add_layer(layer);
        Ok(self)
    }

    /// This layer will make sure that the database is initialized correctly,
    /// e.g.:
    /// - genesis or snapshot recovery will be performed if it's required.
    /// - we perform the storage rollback if required (e.g. if reorg is detected).
    ///
    /// Depending on the `kind` provided, either a task or a precondition will be added.
    ///
    /// *Important*: the task should be added by at most one component, because
    /// it assumes unique control over the database. Multiple components adding this
    /// layer in a distributed mode may result in the database corruption.
    ///
    /// This task works in pair with precondition, which must be present in every component:
    /// the precondition will prevent node from starting until the database is initialized.
    fn add_storage_initialization_layer(mut self, kind: LayerKind) -> anyhow::Result<Self> {
        let config = &self.config.local.snapshot_recovery;
        let snapshot_recovery_config = config.enabled.then_some(SnapshotRecoveryConfig {
            snapshot_l1_batch_override: config.l1_batch,
            drop_storage_key_preimages: config.drop_storage_key_preimages,
            object_store_config: config.object_store.clone(),
        });
        self.node.add_layer(ExternalNodeInitStrategyLayer {
            l2_chain_id: self.config.local.networks.l2_chain_id,
            max_postgres_concurrency: config.postgres.max_concurrency,
            snapshot_recovery_config,
        });
        let mut layer = NodeStorageInitializerLayer::new();
        if matches!(kind, LayerKind::Precondition) {
            layer = layer.as_precondition();
        }
        self.node.add_layer(layer);
        Ok(self)
    }

    pub fn build_for_revert(mut self, l1_batch: L1BatchNumber) -> anyhow::Result<ZkStackService> {
        self = self
            .add_pools_layer()?
            .add_block_reverter_layer()?
            .add_unconditional_revert_layer(l1_batch)?;
        Ok(self.node.build())
    }
}

/// Layers that depend on the remote configuration.
impl ExternalNodeBuilder {
    fn web3_api_optional_config(&self) -> anyhow::Result<Web3ServerOptionalConfig> {
        let config = &self.config.local.api.web3_json_rpc;
        // The refresh interval should be several times lower than the pruning removal delay, so that
        // soft-pruning will timely propagate to the API server.
        let pruning_info_refresh_interval = self.config.local.pruning.removal_delay / 5;

        Ok(Web3ServerOptionalConfig {
            namespaces: config.api_namespaces.clone(),
            filters_limit: config.filters_limit,
            subscriptions_limit: config.subscriptions_limit,
            batch_request_size_limit: config.max_batch_request_size.get(),
            response_body_size_limit: config.max_response_body_size(),
            with_extended_tracing: config.extended_api_tracing,
            pruning_info_refresh_interval,
            polling_interval: config.pubsub_polling_interval,
            request_timeout: config.request_timeout,
            websocket_requests_per_minute_limit: Some(config.websocket_requests_per_minute_limit),
        })
    }

    fn add_settlement_layer_data(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SettlementLayerData::new(
            zksync_gateway_migrator::node::ENConfig {
                l1_specific_contracts: self.config.l1_specific_contracts(),
                l1_chain_contracts: self.config.l1_settelment_contracts(),
                l2_contracts: self.config.l2_contracts(),
                chain_id: self.config.local.networks.l2_chain_id,
                gateway_rpc_url: self.config.local.secrets.l1.gateway_rpc_url.clone(),
            },
        ));
        Ok(self)
    }

    fn add_tx_sender_layer(mut self) -> anyhow::Result<Self> {
        let config = &self.config.local.api.web3_json_rpc;
        let postgres_storage_config = PostgresStorageCachesConfig {
            factory_deps_cache_size: config.factory_deps_cache_size.0,
            initial_writes_cache_size: config.initial_writes_cache_size.0,
            latest_values_cache_size: config.latest_values_cache_size.0,
            latest_values_max_block_lag: config.latest_values_max_block_lag.get(),
        };
        let max_vm_concurrency = config.vm_concurrency_limit;
        let tx_sender_layer = TxSenderLayer::new(
            postgres_storage_config,
            max_vm_concurrency,
            (&self.config).into(),
            self.config.local.timestamp_asserter.clone(),
        )
        .with_whitelisted_tokens_for_aa_cache(true);

        self.node.add_layer(ProxySinkLayer);
        self.node.add_layer(tx_sender_layer);
        Ok(self)
    }

    fn add_http_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let mut optional_config = self.web3_api_optional_config()?;
        // Not relevant for HTTP server, so we reset to prevent a logged warning.
        optional_config.websocket_requests_per_minute_limit = None;
        let internal_api_config_base: InternalApiConfigBase = (&self.config).into();

        self.node.add_layer(Web3ServerLayer::http(
            self.config.local.api.web3_json_rpc.http_port,
            internal_api_config_base,
            optional_config,
        ));

        Ok(self)
    }

    fn add_ws_web3_api_layer(mut self) -> anyhow::Result<Self> {
        // TODO: Support websocket requests per minute limit
        let optional_config = self.web3_api_optional_config()?;
        let internal_api_config_base: InternalApiConfigBase = (&self.config).into();

        self.node.add_layer(Web3ServerLayer::ws(
            self.config.local.api.web3_json_rpc.ws_port,
            internal_api_config_base,
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
            .add_query_eth_client_layer()?
            .add_settlement_layer_data()?
            .add_reorg_detector_layer()?;

        #[cfg(not(target_env = "msvc"))]
        {
            self = self.add_jemalloc_monitor_layer()?;
        }

        // Add layers that must run only on a single component.
        if components.contains(&Component::Core) {
            // Core is a singleton & mandatory component,
            // so until we have a dedicated component for "auxiliary" tasks,
            // it's responsible for things like metrics.
            self = self
                .add_postgres_layer()?
                .add_external_node_metrics_layer()?;
            // We assign the storage initialization to the core, as it's considered to be
            // the "main" component.
            self = self
                .add_block_reverter_layer()?
                .add_storage_initialization_layer(LayerKind::Task)?;
        }

        // Add preconditions for all the components.
        self = self
            .add_validate_chain_ids_layer()?
            .add_storage_initialization_layer(LayerKind::Precondition)?;

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
                        .add_bridge_addresses_updater_layer()?
                        .add_mempool_cache_layer()?
                        .add_tree_api_client_layer()?
                        .add_main_node_fee_params_fetcher_layer()?
                        .add_tx_sender_layer()?
                        .add_http_web3_api_layer()?;
                }
                Component::WsApi => {
                    self = self
                        .add_sync_state_updater_layer()?
                        .add_bridge_addresses_updater_layer()?
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
                    if components.contains(&Component::Tree) {
                        // Do nothing, will be handled by the `Tree` component.
                    } else {
                        self = self.add_isolated_tree_api_layer()?;
                    }
                }
                Component::TreeFetcher => {
                    self = self.add_tree_data_fetcher_layer()?;
                }
                Component::DataAvailabilityFetcher => {
                    self = self
                        .add_da_client_layer()?
                        .add_data_availability_fetcher_layer()?;
                }
                Component::Core => {
                    // Main tasks
                    self = self
                        .add_state_keeper_layer()?
                        .add_consensus_layer()?
                        .add_pruning_layer()?
                        .add_consistency_checker_layer()?
                        .add_commitment_generator_layer()?
                        .add_batch_status_updater_layer()?
                        .add_logs_bloom_backfill_layer()?;
                }
            }
        }

        Ok(self.node.build())
    }
}

/// Marker for layers that can add either a task or a precondition.
#[derive(Debug)]
enum LayerKind {
    Task,
    Precondition,
}
