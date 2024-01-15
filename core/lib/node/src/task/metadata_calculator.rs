use tokio::sync::watch;
use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_storage::RocksDB;

use super::{IntoZkSyncTask, TaskInitError, ZkSyncTask};
use crate::{
    node::NodeContext,
    resource::{
        object_store::ObjectStoreResource, pools::MasterPoolResource,
        stop_receiver::StopReceiverResource,
    },
};

#[derive(Debug)]
pub struct MetadataCalculatorTask {
    metadata_calculator: MetadataCalculator,
    main_pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
}

impl IntoZkSyncTask for MetadataCalculatorTask {
    type Config = MetadataCalculatorConfig;

    fn create(
        node: &NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let pool: MasterPoolResource = node.get_resource(MasterPoolResource::RESOURCE_NAME).ok_or(
            TaskInitError::ResourceLacking(MasterPoolResource::RESOURCE_NAME),
        )?;
        let main_pool = node.runtime_handle().block_on(pool.get()).unwrap();
        let object_store: Option<ObjectStoreResource> =
            node.get_resource(ObjectStoreResource::RESOURCE_NAME); // OK to be None.

        if object_store.is_none() {
            // TODO: use internal logging system?
            tracing::info!(
                "Object store is not configured, metadata calculator will run without it."
            );
        }

        let metadata_calculator = node
            .runtime_handle()
            .block_on(MetadataCalculator::new(config, object_store.map(|os| os.0)));

        let stop_receiver: StopReceiverResource = node
            .get_resource(StopReceiverResource::RESOURCE_NAME)
            .ok_or(TaskInitError::ResourceLacking(
                StopReceiverResource::RESOURCE_NAME,
            ))?;
        Ok(Box::new(Self {
            metadata_calculator,
            main_pool,
            stop_receiver: stop_receiver.0,
        }))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for MetadataCalculatorTask {
    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>> {
        Some(Box::new(self.metadata_calculator.tree_health_check()))
    }

    async fn run(self: Box<Self>) -> anyhow::Result<()> {
        let result = self
            .metadata_calculator
            .run(self.main_pool, self.stop_receiver)
            .await;

        // Wait for all the instances of RocksDB to be destroyed.
        tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
            .await
            .unwrap();

        result
    }
}
