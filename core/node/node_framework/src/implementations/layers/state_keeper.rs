use zksync_config::configs::chain::StateKeeperConfig;

use crate::{
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

pub struct StateKeeperLayer {
    config: StateKeeperConfig,
}

impl StateKeeperLayer {
    pub fn new(config: StateKeeperConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl WiringLayer for StateKeeperLayer {
    fn layer_name(&self) -> &'static str {
        "state_keeper_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get IO
        // Get batch executor base
        // Get FeeParamsProvider or how is it called now
        // ConditionalSealer - lazy?
        // What about mempool task and resource? probably handled by IO resource?
        todo!()
    }
}
