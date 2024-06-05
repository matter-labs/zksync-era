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
        // Note: the EN config doesn't currently support specifying configuration for replicas,
        // so we reuse the master configuration for that purpose.
        // Settings unconditionally set to `None` are either not supported by the EN configuration layer
        // or are not used in the context of the external node.
        let config = PostgresConfig {
            max_connections: self.config.postgres.max_connections,
            max_connections_master: self.config.postgres.max_connections,
            acquire_timeout_sec: None,
            statement_timeout_sec: None,
            long_connection_threshold_ms: None,
            slow_query_threshold_ms: self.config.optional.slow_query_threshold(),
            test_server_url: None,
            test_prover_url: None,
        };
        let secrets = DatabaseSecrets {
            server_url: self.config.postgres.database_url(),
            server_replica_url: self.config.postgres.database_url(),
            prover_url: None,
        };
        let pools_layer = PoolsLayerBuilder::empty(config, secrets)
            .with_master(true)
            .with_replica(true)
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_preconditions(mut self) -> anyhow::Result<Self> {
        todo!()
    }

    pub fn build(mut self, mut components: Vec<Component>) -> anyhow::Result<ZkStackService> {
        // Add "base" layers
        self = self.add_sigint_handler_layer()?.add_pools_layer()?;

        // Add preconditions for all the components.
        self = self.add_preconditions()?;

        // Sort the components, so that the components they may depend on each other are added in the correct order.
        components.sort_unstable_by_key(|component| match component {
            // API consumes the resources provided by other layers (multiple ones), so it has to come the last.
            Component::HttpApi | Component::WsApi => 1,
            // Default priority.
            _ => 0,
        });

        for component in components {
            match component {
                Component::HttpApi => todo!(),
                Component::WsApi => todo!(),
                Component::Tree => todo!(),
                Component::TreeApi => {
                    anyhow::ensure!(
                        components.contains(&Component::Tree),
                        "Merkle tree API cannot be started without a tree component"
                    );
                    // Do nothing, will be handled by the `Tree` component.
                }
                Component::TreeFetcher => todo!(),
                Component::Core => todo!(),
            }
        }

        Ok(self.node.build()?)
    }
}
