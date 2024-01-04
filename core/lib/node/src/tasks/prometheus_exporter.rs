use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::{resources::stop_receiver::StopReceiver, ZkSyncNode, ZkSyncTask};

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

#[async_trait::async_trait]
impl ZkSyncTask for PrometheusExporterTask {
    type Config = PrometheusExporterConfig;

    fn new(node: &ZkSyncNode, config: Self::Config) -> Self {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        let stop_receiver: StopReceiver = node
            .get_resource(crate::resources::stop_receiver::RESOURCE_NAME)
            .unwrap(); // TODO do not unwrap

        Self {
            config,
            prometheus_health_check: Some(prometheus_health_check),
            prometheus_health_updater,
            stop_receiver: stop_receiver.0,
        }
    }

    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>> {
        self.prometheus_health_check
            .take()
            .map(|c| Box::new(c) as Box<dyn CheckHealth>)
    }

    async fn run(self) -> anyhow::Result<()> {
        let prometheus_task = self.config.run(self.stop_receiver);
        self.prometheus_health_updater
            .update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(self.prometheus_health_updater);
        res
    }
}
