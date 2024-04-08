use std::sync::Arc;

use zksync_config::configs::chain::L1BatchCommitDataGeneratorMode;
use zksync_core::eth_sender::l1_batch_commit_data_generator::{
    L1BatchCommitDataGenerator, RollupModeL1BatchCommitDataGenerator,
    ValidiumModeL1BatchCommitDataGenerator,
};

use crate::{
    implementations::resources::l1_batch_commit_data_generator::L1BatchCommitDataGeneratorResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct L1BatchCommitDataGeneratorLayer {
    mode: L1BatchCommitDataGeneratorMode,
}

impl L1BatchCommitDataGeneratorLayer {
    pub fn new(mode: L1BatchCommitDataGeneratorMode) -> Self {
        Self { mode }
    }
}

#[async_trait::async_trait]
impl WiringLayer for L1BatchCommitDataGeneratorLayer {
    fn layer_name(&self) -> &'static str {
        "l1_batch_commit_data_generator"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator> = match self.mode {
            L1BatchCommitDataGeneratorMode::Rollup => {
                Arc::new(RollupModeL1BatchCommitDataGenerator {})
            }
            L1BatchCommitDataGeneratorMode::Validium => {
                Arc::new(ValidiumModeL1BatchCommitDataGenerator {})
            }
        };
        context.insert_resource(L1BatchCommitDataGeneratorResource(
            l1_batch_commit_data_generator,
        ))?;
        Ok(())
    }
}
