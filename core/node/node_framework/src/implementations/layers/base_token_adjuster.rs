use zksync_base_token_adjuster::BaseTokenConversionPersister;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// A layer that wires the Base Token Adjuster task.
#[derive(Debug)]
pub struct BaseTokenAdjusterLayer {
    config: BaseTokenAdjusterConfig,
}

impl BaseTokenAdjusterLayer {
    pub fn new(config: BaseTokenAdjusterConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BaseTokenAdjusterLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_adjuster"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let master_pool = master_pool_resource.get().await?;

        let persister = BaseTokenConversionPersister::new(master_pool, self.config);

        context.add_task(Box::new(persister));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for BaseTokenConversionPersister {
    fn id(&self) -> TaskId {
        "base_token_adjuster".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
