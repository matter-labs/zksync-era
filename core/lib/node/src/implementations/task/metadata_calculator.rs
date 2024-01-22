use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_storage::RocksDB;

use super::healtcheck_server::HealthCheckTask;
use crate::{
    implementations::resource::{object_store::ObjectStoreResource, pools::MasterPoolResource},
    node::{NodeContext, StopReceiver},
    resource::Resource,
    task::{IntoZkSyncTask, TaskInitError, ZkSyncTask},
};

#[derive(Debug)]
pub struct MetadataCalculatorTask {
    metadata_calculator: MetadataCalculator,
    main_pool: ConnectionPool,
}

impl IntoZkSyncTask for MetadataCalculatorTask {
    const NAME: &'static str = "metadata_calculator";
    type Config = MetadataCalculatorConfig;

    fn create(
        mut node: NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let pool =
            node.get_resource::<MasterPoolResource>()
                .ok_or(TaskInitError::ResourceLacking(
                    MasterPoolResource::resource_id(),
                ))?;
        let main_pool = node.runtime_handle().block_on(pool.get()).unwrap();
        let object_store = node.get_resource::<ObjectStoreResource>(); // OK to be None.

        if object_store.is_none() {
            tracing::info!(
                "Object store is not provided, metadata calculator will run without it."
            );
        }

        let metadata_calculator = node
            .runtime_handle()
            .block_on(MetadataCalculator::new(config, object_store.map(|os| os.0)));

        let healthcheck_id = todo!();
        let healthchecks = node.get_resource_collection::<Box<dyn CheckHealth>>(healthcheck_id);
        healthchecks
            .push(Box::new(metadata_calculator.tree_health_check()))
            .expect("Wiring stage");

        Ok(Box::new(Self {
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
