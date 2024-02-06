use zksync_config::configs::chain::{MempoolConfig, StateKeeperConfig};
use zksync_core::state_keeper::ZkSyncStateKeeper;

use crate::{
    implementations::resources::fee_input::FeeInputResource,
    resource::Resource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct MempoolIoLayer {
    mempool_config: MempoolConfig,
}

#[async_trait::async_trait]
impl WiringLayer for MempoolIoLayer {
    fn layer_name(&self) -> &'static str {
        "mempool_io_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let fee_input = context
            .get_resource::<FeeInputResource>()
            .await
            .ok_or(WiringError::ResourceLacking(FeeInputResource::resource_id()))?;

        todo!()
    }
}
