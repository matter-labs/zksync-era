use zksync_config::configs::{
    chain::L1BatchCommitDataGeneratorMode, da_dispatcher::DADispatcherConfig,
};

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct DataAvailabilityDispatcherLayer {
    da_config: DADispatcherConfig,
    l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
}

impl DataAvailabilityDispatcherLayer {
    pub fn new(
        da_config: DADispatcherConfig,
        l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,
    ) -> Self {
        Self {
            da_config,
            l1_batch_commit_data_generator_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityDispatcherLayer {
    fn layer_name(&self) -> &'static str {
        "da_dispatcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let master_pool = master_pool_resource.get().await.unwrap();

        let da_client = zksync_da::new_da_client(self.da_config.clone());

        Ok(())
    }
}
