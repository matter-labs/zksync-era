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
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for external node initialization strategy.
#[derive(Debug)]
pub struct ExternalNodeInitStrategyLayer {
    pub l2_chain_id: L2ChainId,
    pub snapshot_recovery_config: Option<SnapshotRecoveryConfig>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub block_reverter: Option<BlockReverterResource>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub strategy: NodeInitializationStrategyResource,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalNodeInitStrategyLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_node_role_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let MainNodeClientResource(client) = input.main_node_client;
        let AppHealthCheckResource(app_health) = input.app_health;
        let block_reverter = match input.block_reverter {
            Some(reverter) => {
                // If reverter was provided, we intend to be its sole consumer.
                // We don't want multiple components to attempt reverting blocks.
                let reverter = reverter.0.take().ok_or(WiringError::Configuration(
                    "BlockReverterResource is taken".into(),
                ))?;
                Some(reverter)
            }
            None => None,
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
        // We always want to detect reorgs, even if we can't roll them back.
        let block_reverter = Some(Arc::new(ExternalNodeReverter {
            client,
            pool: pool.clone(),
            reverter: block_reverter,
        }) as Arc<dyn RevertStorage>);
        let strategy = NodeInitializationStrategy {
            genesis,
            snapshot_recovery,
            block_reverter,
        };

        Ok(Output {
            strategy: strategy.into(),
        })
    }
}
