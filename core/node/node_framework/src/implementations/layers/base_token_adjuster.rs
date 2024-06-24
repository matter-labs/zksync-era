use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::Core;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_types::Address;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// A layer that wires the Base Token Adjuster task.
#[derive(Debug)]
pub struct BaseTokenAdjusterLayer {
    base_token_l1_address: Option<Address>,
    config: BaseTokenAdjusterConfig,
}

impl BaseTokenAdjusterLayer {
    pub fn new(base_token_l1_address: Option<Address>, config: BaseTokenAdjusterConfig) -> Self {
        Self {
            base_token_l1_address,
            config,
        }
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

        context.add_task(Box::new(BaseTokenAdjusterTask {
            main_pool: master_pool,
            base_token_l1_address: self.base_token_l1_address,
            config: self.config,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct BaseTokenAdjusterTask {
    main_pool: ConnectionPool<Core>,
    base_token_l1_address: Option<Address>,
    config: BaseTokenAdjusterConfig,
}

#[async_trait::async_trait]
impl Task for BaseTokenAdjusterTask {
    fn id(&self) -> TaskId {
        "base_token_adjuster".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let mut adjuster = zksync_base_token_adjuster::MainNodeBaseTokenAdjuster::new(
            self.main_pool,
            self.config,
            self.base_token_l1_address,
        );

        adjuster.run(stop_receiver.0).await
    }
}
