use std::sync::Arc;

use zksync_node_storage_init::{NodeInitializationStrategy, NodeStorageInitializer};

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
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
/// - `NodeInitializationStrategyResource`
///
/// ## Adds tasks
///
/// Depends on the mode, either  `NodeStorageInitializer` or `NodeStorageInitializerPrecondition`
#[derive(Debug, Default)]
pub struct NodeStorageInitializerLayer {
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
        let NodeInitializationStrategyResource(role) = context.get_resource()?;

        let initializer = NodeStorageInitializer::new(role, pool);

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
/// Resource representing the node initialization strategy.
#[derive(Debug, Clone)]
pub struct NodeInitializationStrategyResource(Arc<NodeInitializationStrategy>);

impl Resource for NodeInitializationStrategyResource {
    fn name() -> String {
        "node_initialization_strategy".into()
    }
}
