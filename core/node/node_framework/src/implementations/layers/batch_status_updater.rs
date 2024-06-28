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
    FromContext, IntoContext,
};

#[derive(Debug, FromContext)]
#[context(crate = "crate")]
struct LayerInput {
    pool: PoolResource<MasterPool>,
    client: MainNodeClientResource,
    #[context(default)]
    app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = "crate")]
struct LayerOutput {
    #[context(task)]
    updater: BatchStatusUpdater,
}

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
        let LayerInput {
            pool,
            client,
            app_health,
        } = LayerInput::from_context(&mut context)?;

        let updater = BatchStatusUpdater::new(client.0, pool.get().await?);

        // Insert healthcheck
        app_health
            .0
            .insert_component(updater.health_check())
            .map_err(WiringError::internal)?;

        // Insert task
        let layer_output = LayerOutput { updater };
        layer_output.into_context(&mut context)?;

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
