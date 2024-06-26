use zksync_node_sync::batch_status_updater::BatchStatusUpdater;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `BatchStatusUpdater`, part of the external node.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `MainNodeClientResource`
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// - `BatchStatusUpdater`
#[derive(Debug)]
pub struct BatchStatusUpdaterLayer;

#[async_trait::async_trait]
impl WiringLayer for BatchStatusUpdaterLayer {
    fn layer_name(&self) -> &'static str {
        "batch_status_updater_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context.get_resource::<PoolResource<MasterPool>>()?;
        let MainNodeClientResource(client) = context.get_resource()?;

        let updater = BatchStatusUpdater::new(client, pool.get().await?);

        // Insert healthcheck
        let AppHealthCheckResource(app_health) = context.get_resource_or_default();
        app_health
            .insert_component(updater.health_check())
            .map_err(WiringError::internal)?;

        // Insert task
        context.add_task(updater);

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for BatchStatusUpdater {
    fn id(&self) -> TaskId {
        "batch_status_updater".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
