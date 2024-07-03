use zksync_base_token_adjuster::BaseTokenRatioPersister;
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `BaseTokenRatioPersister`
///
/// Responsible for orchestrating communications with external API feeds to get ETH<->BaseToken
/// conversion ratios and persisting them both in the DB and in the L1.
///
/// ## Requests resources
///
/// - `PoolResource<ReplicaPool>`
///
/// ## Adds tasks
///
/// - `BaseTokenRatioPersister`
#[derive(Debug)]
pub struct BaseTokenRatioPersisterLayer {
    config: BaseTokenAdjusterConfig,
}

impl BaseTokenRatioPersisterLayer {
    pub fn new(config: BaseTokenAdjusterConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BaseTokenRatioPersisterLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_ratio_persister"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;
        let master_pool = master_pool_resource.get().await?;

        let persister = BaseTokenRatioPersister::new(master_pool, self.config);

        context.add_task(persister);

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for BaseTokenRatioPersister {
    fn id(&self) -> TaskId {
        "base_token_ratio_persister".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
