use zksync_health_check::{
    node::AppHealthCheckResource, HealthStatus, HealthUpdater, ReactiveHealthCheck,
};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::prometheus::PrometheusExporterConfig;

/// Wiring layer for Prometheus exporter server.
#[derive(Debug)]
pub struct PrometheusExporterLayer(pub PrometheusExporterConfig);

#[derive(Debug)]
pub struct PrometheusExporterTask {
    config: PrometheusExporterConfig,
    prometheus_health_updater: HealthUpdater,
}

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: PrometheusExporterTask,
}

#[async_trait::async_trait]
impl WiringLayer for PrometheusExporterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "prometheus_exporter"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let (prometheus_health_check, prometheus_health_updater) =
            ReactiveHealthCheck::new("prometheus_exporter");

        input
            .app_health
            .0
            .insert_component(prometheus_health_check)
            .map_err(WiringError::internal)?;

        let task = PrometheusExporterTask {
            config: self.0,
            prometheus_health_updater,
        };

        Ok(Output { task })
    }
}

#[async_trait::async_trait]
impl Task for PrometheusExporterTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "prometheus_exporter".into()
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
