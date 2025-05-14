use std::time::Duration;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::{DbPruner, DbPrunerConfig};

/// Wiring layer for node pruning layer.
#[derive(Debug)]
pub struct PruningLayer {
    pruning_removal_delay: Duration,
    pruning_chunk_size: u32,
    minimum_l1_batch_age: Duration,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub db_pruner: DbPruner,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "pruning_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;

        let db_pruner = DbPruner::new(
            DbPrunerConfig {
                removal_delay: self.pruning_removal_delay,
                pruned_batch_chunk_size: self.pruning_chunk_size,
                minimum_l1_batch_age: self.minimum_l1_batch_age,
            },
            main_pool,
        );

        input
            .app_health
            .0
            .insert_component(db_pruner.health_check())
            .map_err(WiringError::internal)?;
        Ok(Output { db_pruner })
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
