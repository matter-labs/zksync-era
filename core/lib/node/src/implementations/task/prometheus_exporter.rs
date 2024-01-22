use prometheus_exporter::PrometheusExporterConfig;
use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::{
    implementations::resource::healthcheck::HealthCheckResource,
    node::{NodeContext, StopReceiver},
    resource::ResourceCollection,
    task::{IntoZkSyncTask, TaskInitError, ZkSyncTask},
};

#[derive(Debug)]
pub struct PrometheusExporterTask {
    config: PrometheusExporterConfig,
    prometheus_health_updater: HealthUpdater,
}

impl IntoZkSyncTask for PrometheusExporterTask {
    const NAME: &'static str = "prometheus_exporter";
    type Config = PrometheusExporterConfig;

    fn create(
        mut node: NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        let healthchecks =
            node.get_resource_or_default::<ResourceCollection<HealthCheckResource>>();
        healthchecks
            .push(HealthCheckResource::new(prometheus_health_check))
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
