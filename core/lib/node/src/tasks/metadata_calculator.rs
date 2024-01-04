use tokio::sync::watch;
use zksync_config::configs::{chain::OperationsManagerConfig, database::MerkleTreeConfig};
use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;

use crate::{
    resources::{pools::Pools, stop_receiver::StopReceiver},
    ZkSyncNode, ZkSyncTask,
};

#[derive(Debug)]
pub struct MetadataCalculatorTask {
    metadata_calculator: MetadataCalculator,
    main_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl ZkSyncTask for MetadataCalculatorTask {
    type Config = (MerkleTreeConfig, OperationsManagerConfig); // <- wrong

    fn new(node: &ZkSyncNode, config: Self::Config) -> Self {
        let config: MetadataCalculatorConfig = todo!();
        let metadata_calculator = node.block_on(MetadataCalculator::new(config));

        let pools: Pools = node
            .get_resource(crate::resources::pools::RESOURCE_NAME)
            .unwrap(); // TODO do not unwrap
        let main_pool = node.block_on(pools.master_pool()).unwrap(); // TODO do not unwrap

        let stop_receiver: StopReceiver = node
            .get_resource(crate::resources::stop_receiver::RESOURCE_NAME)
            .unwrap(); // TODO do not unwrap
        Self {
            metadata_calculator,
            main_pool,
            stop_receiver: stop_receiver.0,
        }
    }

    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>> {
        Some(Box::new(self.metadata_calculator.tree_health_check()))
    }

    async fn before_launch(&mut self) {
        todo!()
    }

    async fn run(self) -> anyhow::Result<()> {
        self.metadata_calculator
            .run(self.main_pool, self.stop_receiver)
            .await
    }

    async fn after_finish(&mut self) {
        todo!("Wait for rocksdb termination")
    }
}
