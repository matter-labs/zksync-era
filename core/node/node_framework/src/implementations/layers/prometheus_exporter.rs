use prometheus_exporter::PrometheusExporterConfig;
use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::{
    implementations::resources::healthcheck::AppHealthCheckResource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// Builder for a prometheus exporter.
///
/// ## Effects
///
/// - Adds prometheus health check to the `ResourceCollection<HealthCheckResource>`.
/// - Adds `prometheus_exporter` to the node.
#[derive(Debug)]
pub struct PrometheusExporterLayer(pub PrometheusExporterConfig);

#[derive(Debug)]
pub struct PrometheusExporterTask {
    config: PrometheusExporterConfig,
    prometheus_health_updater: HealthUpdater,
}

#[async_trait::async_trait]
impl WiringLayer for PrometheusExporterLayer {
    fn layer_name(&self) -> &'static str {
        "prometheus_exporter"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        let AppHealthCheckResource(app_health) = node.get_resource_or_default().await;
        app_health.insert_component(prometheus_health_check);

        let task = Box::new(PrometheusExporterTask {
            config: self.0,
            prometheus_health_updater,
        });

        node.add_task(task);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for PrometheusExporterTask {
    fn name(&self) -> &'static str {
        "prometheus_exporter"
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
