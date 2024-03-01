use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_dal::ConnectionPool;
use zksync_storage::RocksDB;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource, object_store::ObjectStoreResource,
        pools::MasterPoolResource,
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// Builder for a metadata calculator.
///
/// ## Effects
///
/// - Resolves `MasterPoolResource`.
/// - Resolves `ObjectStoreResource` (optional).
/// - Adds `tree_health_check` to the `ResourceCollection<HealthCheckResource>`.
/// - Adds `metadata_calculator` to the node.
#[derive(Debug)]
pub struct MetadataCalculatorLayer(pub MetadataCalculatorConfig);

#[derive(Debug)]
pub struct MetadataCalculatorTask {
    metadata_calculator: MetadataCalculator,
    main_pool: ConnectionPool,
}

#[async_trait::async_trait]
impl WiringLayer for MetadataCalculatorLayer {
    fn layer_name(&self) -> &'static str {
        "metadata_calculator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context.get_resource::<MasterPoolResource>().await?;
        let main_pool = pool.get().await.unwrap();
        let object_store = context.get_resource::<ObjectStoreResource>().await.ok(); // OK to be None.

        if object_store.is_none() {
            tracing::info!(
                "Object store is not provided, metadata calculator will run without it."
            );
        }

        let metadata_calculator =
            MetadataCalculator::new(self.0, object_store.map(|os| os.0)).await?;

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health.insert_component(metadata_calculator.tree_health_check());

        let task = Box::new(MetadataCalculatorTask {
            metadata_calculator,
            main_pool,
        });
        context.add_task(task);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for MetadataCalculatorTask {
    fn name(&self) -> &'static str {
        "metadata_calculator"
    }

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
