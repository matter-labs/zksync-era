//! This module provides a "builder" for the external node,
//! as well as an interface to run the node with the specified components.

use anyhow::Context as _;
use zksync_config::{
    configs::{api::HealthCheckConfig, DatabaseSecrets},
    PostgresConfig,
};
use zksync_node_api_server::web3::Namespace;
use zksync_node_framework::{
    implementations::layers::{
        consensus::{ConsensusLayer, Mode},
        healtcheck_server::HealthCheckLayer,
        main_node_client::MainNodeClientLayer,
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
        tree_data_fetcher::TreeDataFetcherLayer,
    },
    service::{ZkStackService, ZkStackServiceBuilder},
};

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
        // bytecode compression was enabled. To process these batches (and also for the case where
        // compression will become optional on the sequencer again), EN has to allow txs without bytecode
        // compression.
        const OPTIONAL_BYTECODE_COMPRESSION: bool = true;

        // let wallets = self.wallets.clone();
        // let sk_config = try_load_config!(self.configs.state_keeper_config);
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
        let state_keeper_layer = StateKeeperLayer::new(
            self.config.required.state_cache_path.clone(),
            self.config
                .experimental
                .state_keeper_db_block_cache_capacity(),
            self.config.experimental.state_keeper_db_max_open_files,
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
        let layer = PruningLayer::new(
            self.config.optional.pruning_removal_delay(),
            self.config.optional.pruning_chunk_size,
            self.config.optional.pruning_data_retention(),
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_preconditions(mut self) -> anyhow::Result<Self> {
        todo!()
    }

    fn add_tree_data_fetcher_layer(mut self) -> anyhow::Result<Self> {
        let layer = TreeDataFetcherLayer::new(self.config.remote.diamond_proxy_addr);
        self.node.add_layer(layer);
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
        self = self.add_preconditions()?;

        // Sort the components, so that the components they may depend on each other are added in the correct order.
        components.sort_unstable_by_key(|component| match component {
            // API consumes the resources provided by other layers (multiple ones), so it has to come the last.
            Component::HttpApi | Component::WsApi => 1,
            // Default priority.
            _ => 0,
        });

        for component in &components {
            match component {
                Component::HttpApi => todo!(),
                Component::WsApi => todo!(),
                Component::Tree => {
                    // Right now, distributed mode for EN is not fully supported, e.g. there are some
                    // issues with reorg detection and snapshot recovery.
                    // So we require the core component to be present, e.g. forcing the EN to run in a monolithic mode.
                    anyhow::ensure!(
                        components.contains(&Component::Core),
                        "Tree must run on the same machine as Core"
                    );
                    todo!()
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
                        .add_pruning_layer()?;

                    todo!()
                }
            }
        }

        Ok(self.node.build()?)
    }
}
