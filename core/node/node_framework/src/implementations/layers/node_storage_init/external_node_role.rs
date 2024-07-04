use std::sync::Arc;

// Re-export to initialize the layer without having to depend on the crate directly.
pub use zksync_node_storage_init::SnapshotRecoveryConfig;
use zksync_node_storage_init::{
    external_node::{ExternalNodeGenesis, ExternalNodeReverter, ExternalNodeSnapshotRecovery},
    InitializeStorage, NodeInitializationStrategy, RevertStorage,
};
use zksync_types::L2ChainId;

use super::NodeInitializationStrategyResource;
use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        reverter::BlockReverterResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for external node initialization strategy.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `MainNodeClientResource`
/// - `BlockReverterResource` (optional)
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds resources
///
/// - `NodeInitializationStrategyResource`
#[derive(Debug)]
pub struct ExternalNodeInitStrategyLayer {
    pub l2_chain_id: L2ChainId,
    pub snapshot_recovery_config: Option<SnapshotRecoveryConfig>,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalNodeInitStrategyLayer {
    fn layer_name(&self) -> &'static str {
        "external_node_role_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context
            .get_resource::<PoolResource<MasterPool>>()?
            .get()
            .await?;
        let MainNodeClientResource(client) = context.get_resource()?;
        let AppHealthCheckResource(app_health) = context.get_resource_or_default();
        let block_reverter = match context.get_resource::<BlockReverterResource>() {
            Ok(reverter) => {
                // If reverter was provided, we intend to be its sole consumer.
                // We don't want multiple components to attempt reverting blocks.
                let reverter = reverter.0.take().ok_or(WiringError::Configuration(
                    "BlockReverterResource is taken".into(),
                ))?;
                Some(reverter)
            }
            Err(WiringError::ResourceLacking { .. }) => None,
            Err(err) => return Err(err),
        };

        let genesis = Arc::new(ExternalNodeGenesis {
            l2_chain_id: self.l2_chain_id,
            client: client.clone(),
            pool: pool.clone(),
        });
        let snapshot_recovery = self.snapshot_recovery_config.map(|recovery_config| {
            Arc::new(ExternalNodeSnapshotRecovery {
                client: client.clone(),
                pool: pool.clone(),
                recovery_config,
                app_health,
            }) as Arc<dyn InitializeStorage>
        });
        let block_reverter = block_reverter.map(|block_reverter| {
            Arc::new(ExternalNodeReverter {
                client,
                pool: pool.clone(),
                reverter: block_reverter,
            }) as Arc<dyn RevertStorage>
        });
        let strategy = NodeInitializationStrategy {
            genesis,
            snapshot_recovery,
            block_reverter,
        };

        context.insert_resource(NodeInitializationStrategyResource(strategy))?;
        Ok(())
    }
}
