use std::{num::NonZeroUsize, sync::Arc};

use zksync_block_reverter::node::BlockReverterResource;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::L2ChainId;
use zksync_web3_decl::node::MainNodeClientResource;

use super::NodeInitializationStrategyResource;
use crate::{
    external_node::{ExternalNodeGenesis, ExternalNodeReverter, ExternalNodeSnapshotRecovery},
    InitializeStorage, NodeInitializationStrategy, RevertStorage, SnapshotRecoveryConfig,
};

/// Wiring layer for external node initialization strategy.
#[derive(Debug)]
pub struct ExternalNodeInitStrategyLayer {
    pub l2_chain_id: L2ChainId,
    pub max_postgres_concurrency: NonZeroUsize,
    pub snapshot_recovery_config: Option<SnapshotRecoveryConfig>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub block_reverter: Option<BlockReverterResource>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
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
        let snapshot_recovery = match self.snapshot_recovery_config {
            Some(recovery_config) => {
                // Add a connection for checking whether the storage is initialized.
                let recovery_pool = input
                    .master_pool
                    .get_custom(self.max_postgres_concurrency.get() as u32 + 1)
                    .await?;
                let recovery: Arc<dyn InitializeStorage> = Arc::new(ExternalNodeSnapshotRecovery {
                    client: client.clone(),
                    pool: recovery_pool,
                    max_concurrency: self.max_postgres_concurrency,
                    recovery_config,
                    app_health,
                });
                Some(recovery)
            }
            None => None,
        };
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
