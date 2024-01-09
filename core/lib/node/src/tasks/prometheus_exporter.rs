use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::{
    resources::stop_receiver::StopReceiverResource, IntoZkSyncTask, ZkSyncNode, ZkSyncTask,
};

use super::TaskInitError;

/** Reference
   let prom_config = configs
       .prometheus_config
       .clone()
       .context("prometheus_config")?;
   let prom_config = PrometheusExporterConfig::pull(prom_config.listener_port);

   let (prometheus_health_check, prometheus_health_updater) =
       ReactiveHealthCheck::new("prometheus_exporter");
   healthchecks.push(Box::new(prometheus_health_check));
   let prometheus_task = prom_config.run(stop_receiver.clone());
   let prometheus_task = tokio::spawn(async move {
       prometheus_health_updater.update(HealthStatus::Ready.into());
       let res = prometheus_task.await;
       drop(prometheus_health_updater);
       res
   });
*/

#[derive(Debug)]
pub struct PrometheusExporterTask {
    config: PrometheusExporterConfig,
    prometheus_health_check: Option<ReactiveHealthCheck>,
    prometheus_health_updater: HealthUpdater,
    stop_receiver: watch::Receiver<bool>,
}

impl IntoZkSyncTask for PrometheusExporterTask {
    type Config = PrometheusExporterConfig;

    fn create(
        node: &ZkSyncNode,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, TaskInitError> {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        let stop_receiver: StopReceiverResource = node
            .get_resource(crate::resources::stop_receiver::RESOURCE_NAME)
            .ok_or(TaskInitError::ResourceLacking(
                crate::resources::stop_receiver::RESOURCE_NAME,
            ))?;

        Ok(Box::new(Self {
            config,
            prometheus_health_check: Some(prometheus_health_check),
            prometheus_health_updater,
            stop_receiver: stop_receiver.0,
        }))
    }
}

#[async_trait::async_trait]
impl ZkSyncTask for PrometheusExporterTask {
    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>> {
        self.prometheus_health_check
            .take()
            .map(|c| Box::new(c) as Box<dyn CheckHealth>)
    }

    async fn run(self: Box<Self>) -> anyhow::Result<()> {
        let prometheus_task = self.config.run(self.stop_receiver);
        self.prometheus_health_updater
            .update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(self.prometheus_health_updater);
        res
    }
}
