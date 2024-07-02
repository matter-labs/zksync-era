use std::sync::Arc;

use zksync_node_storage_init::{NodeRole, NodeStorageInitializer, SnapshotRecoveryConfig};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
        reverter::BlockReverterResource,
    },
    resource::Resource,
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
};

pub mod external_node_role;
pub mod main_node_role;

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
    pub fn as_precondition(mut self) -> Self {
        self.as_precondition = true;
        self
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
        if self.as_precondition {
            return "node_storage_initializer_precondition_layer";
        }
        "node_storage_initializer_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context
            .get_resource::<PoolResource<MasterPool>>()?
            .get()
            .await?;
        let NodeRoleResource(role) = context.get_resource()?;
        let AppHealthCheckResource(app_health) = context.get_resource_or_default();

        let mut initializer = NodeStorageInitializer::new(role, pool, app_health)
            .with_recovery_config(self.snapshot_recovery_config);

        // We don't want to give precondition access to the block reverter, just in case.
        if !self.as_precondition {
            let block_reverter = match context.get_resource::<BlockReverterResource>() {
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
            context.add_task(NodeStorageInitializerPrecondition(initializer));
        } else {
            context.add_task(initializer);
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
        tracing::info!("Starting the node storage initialization task");
        (*self).run(stop_receiver.0).await?;
        tracing::info!("Node storage initialization task completed");
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

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tracing::info!("Waiting for node storage to be initialized");
        let result = self.0.wait_for_initialized_storage(stop_receiver.0).await;
        tracing::info!("Node storage initialization precondition completed");
        result
    }
}

// Note: unlike with other modules, this one keeps within the same file to simplify
// moving the implementations out of the framework soon.
/// Resource representing the node role.
/// Used for storage initialization.
#[derive(Debug, Clone)]
pub struct NodeRoleResource(Arc<dyn NodeRole>);

impl Resource for NodeRoleResource {
    fn name() -> String {
        "node_role".into()
    }
}
