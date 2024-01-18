use prometheus_exporter::PrometheusExporterConfig;
use zksync_health_check::{CheckHealth, HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::node::{NodeContext, StopReceiver};
use crate::task::{IntoZkSyncTask, TaskInitError, ZkSyncTask};

use super::healtcheck_server::HealthCheckTask;

#[derive(Debug)]
pub struct PrometheusExporterTask {
    config: PrometheusExporterConfig,
    prometheus_health_updater: HealthUpdater,
}

impl IntoZkSyncTask for PrometheusExporterTask {
    const NAME: &'static str = "prometheus_exporter";
    type Config = PrometheusExporterConfig;

    fn create(
        node: &NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        let healthchecks = node.get_resource_collection::<Box<dyn CheckHealth>>(
            HealthCheckTask::HEALTHCHECK_COLLECTION_NAME,
        );
        healthchecks
            .push(Box::new(prometheus_health_check))
            .expect("Wiring stage");

        Ok(Box::new(Self {
            config,
            prometheus_health_updater,
        }))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for PrometheusExporterTask {
    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let prometheus_task = self.config.run(stop_receiver.0);
        self.prometheus_health_updater
            .update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(self.prometheus_health_updater);
        res
    }
}
