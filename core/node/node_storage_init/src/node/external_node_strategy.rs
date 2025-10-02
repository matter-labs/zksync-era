use std::{num::NonZeroUsize, sync::Arc};

use zksync_block_reverter::node::BlockReverterResource;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_l1_recovery::BlobClientResource;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};
use zksync_shared_resources::contracts::SettlementLayerContractsResource;
use zksync_types::L2ChainId;
use zksync_web3_decl::client::{DynClient, L1, L2};

use crate::{
    external_node::{ExternalNodeGenesis, ExternalNodeReverter, NodeRecovery},
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
    l1_client: Box<DynClient<L1>>,
    master_pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
    block_reverter: Option<BlockReverterResource>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
    blob_client: Option<BlobClientResource>,
    contracts: SettlementLayerContractsResource,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalNodeInitStrategyLayer {
    type Input = Input;
    type Output = NodeInitializationStrategy;

    fn layer_name(&self) -> &'static str {
        "external_node_role_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let client = input.main_node_client;
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
                let recovery: Arc<dyn InitializeStorage> = Arc::new(NodeRecovery {
                    main_node_client: Some(client.clone()),
                    l1_client: input.l1_client.clone(),
                    pool: recovery_pool,
                    max_concurrency: self.max_postgres_concurrency,
                    recovery_config,
                    app_health: input.app_health,
                    diamond_proxy_addr: input.contracts.0.chain_contracts_config.diamond_proxy_addr,
                    blob_client: input.blob_client.clone().map(|x| x.0),
                });
                Some(recovery)
            }
            None => None,
        };
        // We always want to detect reorgs, even if we can't roll them back.
        let block_reverter = ExternalNodeReverter {
            client,
            pool,
            reverter: block_reverter,
        };
        let block_reverter = Some(Arc::new(block_reverter) as Arc<dyn RevertStorage>);

        Ok(NodeInitializationStrategy {
            genesis,
            snapshot_recovery,
            block_reverter,
        })
    }
}
