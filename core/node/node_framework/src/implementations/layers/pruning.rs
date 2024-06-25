use std::time::Duration;

use zksync_node_db_pruner::{DbPruner, DbPrunerConfig};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for node pruning layer.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// - `DbPruner`
#[derive(Debug)]
pub struct PruningLayer {
    pruning_removal_delay: Duration,
    pruning_chunk_size: u32,
    minimum_l1_batch_age: Duration,
}

impl PruningLayer {
    pub fn new(
        pruning_removal_delay: Duration,
        pruning_chunk_size: u32,
        minimum_l1_batch_age: Duration,
    ) -> Self {
        Self {
            pruning_removal_delay,
            pruning_chunk_size,
            minimum_l1_batch_age,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PruningLayer {
    fn layer_name(&self) -> &'static str {
        "pruning_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let main_pool = pool_resource.get().await?;

        let db_pruner = DbPruner::new(
            DbPrunerConfig {
                removal_delay: self.pruning_removal_delay,
                pruned_batch_chunk_size: self.pruning_chunk_size,
                minimum_l1_batch_age: self.minimum_l1_batch_age,
            },
            main_pool,
        );

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health
            .insert_component(db_pruner.health_check())
            .map_err(WiringError::internal)?;

        context.add_task(Box::new(db_pruner));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for DbPruner {
    fn id(&self) -> TaskId {
        "db_pruner".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
