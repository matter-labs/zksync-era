use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_prometheus_exporter::PrometheusExporterConfig;

use crate::{
    implementations::resources::healthcheck::AppHealthCheckResource,
    service::{ServiceContext, StopReceiver},
    task::{TaskId, UnconstrainedTask},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for Prometheus exporter server.
///
/// ## Requests resources
///
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// - `PrometheusExporterTask`
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
        app_health
            .insert_component(prometheus_health_check)
            .map_err(WiringError::internal)?;

        let task = Box::new(PrometheusExporterTask {
            config: self.0,
            prometheus_health_updater,
        });

        node.add_unconstrained_task(task);
        Ok(())
    }
}

#[async_trait::async_trait]
impl UnconstrainedTask for PrometheusExporterTask {
    fn id(&self) -> TaskId {
        "prometheus_exporter".into()
    }

    async fn run_unconstrained(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let prometheus_task = self.config.run(stop_receiver.0);
        self.prometheus_health_updater
            .update(HealthStatus::Ready.into());
        let res = prometheus_task.await;
        drop(self.prometheus_health_updater);
        res
    }
}
