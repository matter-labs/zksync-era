use zksync_config::configs::house_keeper::HouseKeeperConfig;
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::{blocks_state_reporter::BlockMetricsReporter, periodic_job::PeriodicJob};

/// Wiring layer for `HouseKeeper` - a component responsible for managing prover jobs
/// and auxiliary server activities.
#[derive(Debug)]
pub struct HouseKeeperLayer {
    house_keeper_config: HouseKeeperConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    replica_pool: PoolResource<ReplicaPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub l1_batch_metrics_reporter: BlockMetricsReporter,
}

impl HouseKeeperLayer {
    pub fn new(house_keeper_config: HouseKeeperConfig) -> Self {
        Self {
            house_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for HouseKeeperLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "house_keeper_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Initialize resources
        let replica_pool = input.replica_pool.get().await?;

        // Initialize and add tasks
        let l1_batch_metrics_reporter = BlockMetricsReporter::new(
            self.house_keeper_config
                .l1_batch_metrics_reporting_interval,
            replica_pool,
        );

        Ok(Output {
            l1_batch_metrics_reporter,
        })
    }
}

#[async_trait::async_trait]
impl Task for BlockMetricsReporter {
    fn id(&self) -> TaskId {
        "l1_batch_metrics_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
