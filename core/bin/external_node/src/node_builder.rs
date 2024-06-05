//! This module provides a "builder" for the external node,
//! as well as an interface to run the node with the specified components.

use zksync_config::{configs::DatabaseSecrets, PostgresConfig};
use zksync_node_framework::{
    implementations::layers::{pools_layer::PoolsLayerBuilder, sigint::SigintHandlerLayer},
    service::{ZkStackService, ZkStackServiceBuilder},
};

use crate::{config::ExternalNodeConfig, Component};

/// Builder for the external node.
#[derive(Debug)]
pub(crate) struct ExternalNodeBuilder {
    node: ZkStackServiceBuilder,
    config: ExternalNodeConfig,
}

impl ExternalNodeBuilder {
    pub fn new(node: ZkStackServiceBuilder, config: ExternalNodeConfig) -> Self {
        Self { node, config }
    }

    fn add_sigint_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SigintHandlerLayer);
        Ok(self)
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        let config = PostgresConfig {
            max_connections: todo!(),
            max_connections_master: todo!(),
            acquire_timeout_sec: todo!(),
            statement_timeout_sec: todo!(),
            long_connection_threshold_ms: todo!(),
            slow_query_threshold_ms: todo!(),
            test_server_url: None,
            test_prover_url: None,
        };
        let secrets = DatabaseSecrets {
            server_url: todo!(),
            server_replica_url: todo!(),
            prover_url: None,
        };
        let pools_layer = PoolsLayerBuilder::empty(config, secrets)
            .with_master(true)
            .with_replica(true)
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    pub fn build(mut self, mut components: Vec<Component>) -> anyhow::Result<ZkStackService> {
        todo!()
    }
}
