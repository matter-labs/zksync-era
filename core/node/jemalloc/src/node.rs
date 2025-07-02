//! Node framework integration for [`JemallocMonitor`].

use std::sync::Arc;

use zksync_health_check::{async_trait, AppHealthCheck};
use zksync_node_framework::{
    FromContext, IntoContext, StopReceiver, Task, TaskId, WiringError, WiringLayer,
};

use crate::JemallocMonitor;

#[derive(Debug)]
pub struct JemallocMonitorLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    monitor: JemallocMonitor,
}

#[async_trait]
impl WiringLayer for JemallocMonitorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "jemalloc_monitor_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let monitor = JemallocMonitor::new()?;
        input
            .app_health
            .insert_component(monitor.health_check())
            .map_err(WiringError::internal)?;
        Ok(Output { monitor })
    }
}

#[async_trait]
impl Task for JemallocMonitor {
    fn id(&self) -> TaskId {
        "jemalloc_monitor_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
