use prometheus_exporter::PrometheusExporterConfig;
use zksync_health_check::{CheckHealth, HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::node::{NodeContext, StopReceiver};
use crate::task::{IntoZkSyncTask, TaskInitError, ZkSyncTask};

#[derive(Debug)]
pub struct PrometheusExporterTask {
    config: PrometheusExporterConfig,
    prometheus_health_check: Option<ReactiveHealthCheck>,
    prometheus_health_updater: HealthUpdater,
}

impl IntoZkSyncTask for PrometheusExporterTask {
    type Config = PrometheusExporterConfig;

    fn create(
        _node: &NodeContext<'_>,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        Ok(Box::new(Self {
            config,
            prometheus_health_check: Some(prometheus_health_check),
            prometheus_health_updater,
        }))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for PrometheusExporterTask {
    fn healthcheck(&mut self) -> Option<Box<dyn CheckHealth>> {
        self.prometheus_health_check
            .take()
            .map(|c| Box::new(c) as Box<dyn CheckHealth>)
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let prometheus_task = self.config.run(stop_receiver.0);
        self.prometheus_health_updater
            .update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(self.prometheus_health_updater);
        res
    }
}
