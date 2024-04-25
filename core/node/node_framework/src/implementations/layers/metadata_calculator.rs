use std::sync::Arc;

use anyhow::Context as _;
use zksync_core::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use zksync_storage::RocksDB;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        object_store::ObjectStoreResource,
        pools::{MasterPoolResource, ReplicaPoolResource},
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
}

#[async_trait::async_trait]
impl WiringLayer for MetadataCalculatorLayer {
    fn layer_name(&self) -> &'static str {
        "metadata_calculator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context.get_resource::<MasterPoolResource>().await?;
        let main_pool = pool.get().await?;
        // The number of connections in a recovery pool is based on the mainnet recovery runs. It doesn't need
        // to be particularly accurate at this point, since the main node isn't expected to recover from a snapshot.
        let recovery_pool = context
            .get_resource::<ReplicaPoolResource>()
            .await?
            .get_custom(10)
            .await?;

        let object_store = context.get_resource::<ObjectStoreResource>().await.ok(); // OK to be None.
        if object_store.is_none() {
            tracing::info!(
                "Object store is not provided, metadata calculator will run without it."
            );
        }

        let metadata_calculator = MetadataCalculator::new(
            self.0,
            object_store.map(|store_resource| store_resource.0),
            main_pool,
        )
        .await?
        .with_recovery_pool(recovery_pool);

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health.insert_custom_component(Arc::new(metadata_calculator.tree_health_check()));

        let task = Box::new(MetadataCalculatorTask {
            metadata_calculator,
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
        let result = self.metadata_calculator.run(stop_receiver.0).await;

        // Wait for all the instances of RocksDB to be destroyed.
        tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
            .await
            .context("failed terminating RocksDB instances")?;
        result
    }
}
