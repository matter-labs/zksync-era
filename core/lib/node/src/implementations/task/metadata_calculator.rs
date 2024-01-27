use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_dal::ConnectionPool;
use zksync_storage::RocksDB;

use crate::{
    implementations::resource::{
        healthcheck::HealthCheckResource, object_store::ObjectStoreResource,
        pools::MasterPoolResource,
    },
    node::{NodeContext, StopReceiver},
    resource::{Resource, ResourceCollection},
    task::{IntoZkSyncTask, TaskInitError, ZkSyncTask},
};

/// Builder for a metadata calculator.
#[derive(Debug)]
pub struct MetadataCalculatorTaskBuilder(pub MetadataCalculatorConfig);

#[derive(Debug)]
pub struct MetadataCalculatorTask {
    metadata_calculator: MetadataCalculator,
    main_pool: ConnectionPool,
}

#[async_trait::async_trait]
impl IntoZkSyncTask for MetadataCalculatorTaskBuilder {
    fn task_name(&self) -> &'static str {
        "metadata_calculator"
    }

    async fn create(
        self: Box<Self>,
        mut node: NodeContext<'_>,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let pool = node.get_resource::<MasterPoolResource>().await.ok_or(
            TaskInitError::ResourceLacking(MasterPoolResource::resource_id()),
        )?;
        let main_pool = pool.get().await.unwrap();
        let object_store = node.get_resource::<ObjectStoreResource>().await; // OK to be None.

        if object_store.is_none() {
            tracing::info!(
                "Object store is not provided, metadata calculator will run without it."
            );
        }

        let metadata_calculator =
            MetadataCalculator::new(self.0, object_store.map(|os| os.0)).await?;

        let healthchecks = node
            .get_resource_or_default::<ResourceCollection<HealthCheckResource>>()
            .await;
        healthchecks
            .push(HealthCheckResource::new(
                metadata_calculator.tree_health_check(),
            ))
            .expect("Wiring stage");

        Ok(Box::new(MetadataCalculatorTask {
            metadata_calculator,
            main_pool,
        }))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for MetadataCalculatorTask {
    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let result = self
            .metadata_calculator
            .run(self.main_pool, stop_receiver.0)
            .await;

        // Wait for all the instances of RocksDB to be destroyed.
        tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
            .await
            .unwrap();

        result
    }
}
