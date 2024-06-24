use zksync_config::{configs::base_token_adjuster::BaseTokenAdjusterConfig, ContractsConfig};
use zksync_dal::Core;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_types::L1_ETH_CONTRACT_ADDRESS;

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
    contracts_config: ContractsConfig,
}

impl BaseTokenAdjusterLayer {
    pub fn new(config: BaseTokenAdjusterConfig, contracts_config: ContractsConfig) -> Self {
        Self {
            config,
            contracts_config,
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
            config: self.config,
            contracts_config: self.contracts_config,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct BaseTokenAdjusterTask {
    main_pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    contracts_config: ContractsConfig,
}

#[async_trait::async_trait]
impl Task for BaseTokenAdjusterTask {
    fn id(&self) -> TaskId {
        "base_token_adjuster".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let mut is_eth = true;
        if let Some(base_token_addr) = self.contracts_config.base_token_addr {
            if base_token_addr != L1_ETH_CONTRACT_ADDRESS {
                is_eth = false;
            }
        }

        let mut adjuster = zksync_base_token_adjuster::MainNodeBaseTokenAdjuster::new(
            self.main_pool,
            self.config,
            is_eth,
        );

        adjuster.run(stop_receiver.0).await
    }
}
