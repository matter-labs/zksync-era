use std::time::Duration;

use zksync_node_storage_init::{NodeRole, NodeStorageInitializer, SnapshotRecoveryConfig};

// Note: unlike with other modules, this one keeps
use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        reverter::BlockReverterResource,
    },
    resource::{Resource, Unique},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `NodeStorageInializer`.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `NodeRoleResource`
/// - `BlockReverterResource` (optional)
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds tasks
///
/// Depends on the mode, either  `NodeStorageInitializer` or `NodeStorageInitializerPrecondition`
#[derive(Debug, Default)]
pub struct NodeStorageInitializerLayer {
    snapshot_recovery_config: Option<SnapshotRecoveryConfig>,
    as_precondition: bool,
}

impl NodeStorageInitializerLayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Changes the wiring logic to treat the initializer as a precondition.
    pub fn as_precondition() -> Self {
        Self {
            as_precondition: true,
            ..Default::default()
        }
    }

    pub fn with_snapshot_recovery_config(
        mut self,
        snapshot_recovery_config: SnapshotRecoveryConfig,
    ) -> Self {
        self.snapshot_recovery_config = Some(snapshot_recovery_config);
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for NodeStorageInitializerLayer {
    fn layer_name(&self) -> &'static str {
        "batch_status_updater_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context
            .get_resource::<PoolResource<MasterPool>>()
            .await?
            .get()
            .await?;
        let NodeRoleResource(role) = context.get_resource().await?;
        let role = role.take().ok_or(WiringError::Configuration(
            "NodeRoleResource is taken".into(),
        ))?;
        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;

        let mut initializer = NodeStorageInitializer::new(role, pool, app_health)
            .with_recovery_config(self.snapshot_recovery_config);

        // We don't want to give precondition access to the block reverter, just in case.
        if !self.as_precondition {
            let block_reverter = match context.get_resource::<BlockReverterResource>().await {
                Ok(reverter) => {
                    // If reverter was provided, we intend to be its sole consumer.
                    // We don't want multiple components to attempt reverting blocks.
                    let reverter = reverter.0.take().ok_or(WiringError::Configuration(
                        "BlockReverterResource is taken".into(),
                    ))?;
                    Some(reverter)
                }
                Err(WiringError::ResourceLacking { .. }) => None,
                Err(err) => return Err(err),
            };
            initializer = initializer.with_block_reverter(block_reverter);
        }

        // Insert either task or precondition.
        if self.as_precondition {
            context.add_task(Box::new(NodeStorageInitializerPrecondition(initializer)));
        } else {
            context.add_task(Box::new(initializer));
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for NodeStorageInitializer {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedOneshotTask
    }

    fn id(&self) -> TaskId {
        "node_storage_initializer".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}

#[derive(Debug)]
struct NodeStorageInitializerPrecondition(NodeStorageInitializer);

#[async_trait::async_trait]
impl Task for NodeStorageInitializerPrecondition {
    fn kind(&self) -> TaskKind {
        TaskKind::Precondition
    }

    fn id(&self) -> TaskId {
        "node_storage_initializer_precondition".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        const POLLING_INTERVAL: Duration = Duration::from_secs(1);

        let initializer = self.0;
        loop {
            if *stop_receiver.0.borrow() {
                break;
            }

            if initializer
                .storage_initialized(stop_receiver.0.clone())
                .await?
            {
                tracing::info!("Storage is initialized");
                break;
            }

            tokio::time::timeout(POLLING_INTERVAL, stop_receiver.0.changed())
                .await
                .ok();
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NodeRoleResource(Unique<Box<dyn NodeRole>>);

impl Resource for NodeRoleResource {
    fn name() -> String {
        "node_role".into()
    }
}
