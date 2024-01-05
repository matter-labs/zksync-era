use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::{
    chain::OperationsManagerConfig, database::MerkleTreeConfig, object_store,
};
use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_object_store::ObjectStore;

use crate::{
    resources::{
        object_store::ObjectStoreResource, pools::PoolsResource,
        stop_receiver::StopReceiverResource,
    },
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
    type Config = MetadataCalculatorConfig;

    fn new(node: &ZkSyncNode, config: Self::Config) -> Self {
        let pools: PoolsResource = node
            .get_resource(crate::resources::pools::RESOURCE_NAME)
            .unwrap(); // TODO do not unwrap
        let main_pool = node.block_on(pools.master_pool()).unwrap(); // TODO do not unwrap
        let object_store: Option<ObjectStoreResource> =
            node.get_resource(crate::resources::object_store::RESOURCE_NAME); // OK to be None.

        if object_store.is_none() {
            // TODO: use internal logging system?
            tracing::info!(
                "Object store is not configured, metadata calculator will run without it."
            );
        }

        let metadata_calculator = node.block_on(MetadataCalculator::new(config, object_store.0));

        let stop_receiver: StopReceiverResource = node
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
