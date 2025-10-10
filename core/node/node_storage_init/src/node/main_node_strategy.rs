use std::{num::NonZeroUsize, sync::Arc};

use zksync_config::{GenesisConfig, ObjectStoreConfig};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_l1_recovery::BlobClientResource;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};
use zksync_shared_resources::contracts::SettlementLayerContractsResource;
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    external_node::NodeRecovery, main_node::MainNodeGenesis, InitializeStorage,
    NodeInitializationStrategy, SnapshotRecoveryConfig,
};

/// Wiring layer for main node initialization strategy.
#[derive(Debug)]
pub struct MainNodeInitStrategyLayer {
    pub genesis: GenesisConfig,
    pub event_expiration_blocks: u64,
    pub l1_recovery_enabled: bool,
    pub max_postgres_concurrency: NonZeroUsize,
    pub object_store_config: Option<ObjectStoreConfig>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    l1_client: Box<DynClient<L1>>,
    contracts: SettlementLayerContractsResource,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
    blob_client: Option<BlobClientResource>,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeInitStrategyLayer {
    type Input = Input;
    type Output = NodeInitializationStrategy;

    fn layer_name(&self) -> &'static str {
        "main_node_role_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let genesis = Arc::new(MainNodeGenesis {
            contracts: input.contracts.0.clone(),
            genesis: self.genesis,
            event_expiration_blocks: self.event_expiration_blocks,
            l1_client: input.l1_client.clone(),
            pool,
        });

        let snapshot_recovery: Option<Arc<dyn InitializeStorage>> = if self.l1_recovery_enabled {
            let recovery_config = SnapshotRecoveryConfig {
                snapshot_l1_batch_override: None,
                drop_storage_key_preimages: true,
                object_store_config: self.object_store_config,
                recover_from_l1: true,
                recover_main_node_components: true,
            };
            let recovery_pool = input
                .master_pool
                .get_custom(self.max_postgres_concurrency.get() as u32 + 1)
                .await?;
            Some(Arc::new(NodeRecovery {
                main_node_client: None,
                l1_client: input.l1_client.clone(),
                pool: recovery_pool,
                max_concurrency: self.max_postgres_concurrency,
                recovery_config,
                app_health: input.app_health,
                diamond_proxy_addr: input.contracts.0.chain_contracts_config.diamond_proxy_addr,
                blob_client: input.blob_client.clone().map(|x| x.0),
            }))
        } else {
            None
        };

        dbg!(&snapshot_recovery.is_some());
        Ok(NodeInitializationStrategy {
            genesis,
            snapshot_recovery,
            block_reverter: None,
        })
    }
}
