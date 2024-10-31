use std::sync::Arc;

use zksync_config::configs::house_keeper::HouseKeeperConfig;
use zksync_health_check::ReactiveHealthCheck;
use zksync_house_keeper::{
    blocks_state_reporter::L1BatchMetricsReporter, database::DatabaseHealthTask,
    periodic_job::PeriodicJob, version::NodeVersionInfo,
};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{PoolResource, ReplicaPool},
    },
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for `HouseKeeper` - a component responsible for managing prover jobs
/// and auxiliary server activities.
#[derive(Debug)]
pub struct HouseKeeperLayer {
    house_keeper_config: HouseKeeperConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub l1_batch_metrics_reporter: L1BatchMetricsReporter,
    #[context(task)]
    pub database_health_task: DatabaseHealthTask,
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
        let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
            self.house_keeper_config
                .l1_batch_metrics_reporting_interval_ms,
            replica_pool.clone(),
        );

        let app_health = input.app_health.0;
        app_health
            .insert_custom_component(Arc::new(NodeVersionInfo::default()))
            .map_err(WiringError::internal)?;

        let (database_health_check, database_health_updater) =
            ReactiveHealthCheck::new("database_health");

        app_health
            .insert_component(database_health_check)
            .map_err(WiringError::internal)?;

        let database_health_task = DatabaseHealthTask {
            connection_pool: replica_pool.clone(),
            database_health_updater,
        };

        Ok(Output {
            l1_batch_metrics_reporter,
            database_health_task,
        })
    }
}

#[async_trait::async_trait]
impl Task for L1BatchMetricsReporter {
    fn id(&self) -> TaskId {
        "l1_batch_metrics_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for DatabaseHealthTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "database_health".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
