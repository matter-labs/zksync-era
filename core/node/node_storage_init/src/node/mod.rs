use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    resource::Resource,
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::try_stoppable;

use crate::{NodeInitializationStrategy, NodeStorageInitializer};

pub mod external_node_strategy;
pub mod main_node_strategy;

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
    stop_node_on_completion: bool,
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

    pub fn stop_node_on_completion(mut self) -> Self {
        self.stop_node_on_completion = true;
        self
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    strategy: NodeInitializationStrategy,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    initializer: Option<NodeStorageInitializer>,
    #[context(task)]
    precondition: Option<NodeStorageInitializerPrecondition>,
}

impl Output {
    fn initializer(initializer: NodeStorageInitializer) -> Self {
        Self {
            initializer: Some(initializer),
            precondition: None,
        }
    }

    fn precondition(precondition: NodeStorageInitializer) -> Self {
        Self {
            initializer: None,
            precondition: Some(NodeStorageInitializerPrecondition(precondition)),
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for NodeStorageInitializerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        if self.as_precondition {
            return "node_storage_initializer_precondition_layer";
        }
        "node_storage_initializer_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let initializer =
            NodeStorageInitializer::new(input.strategy, pool, self.stop_node_on_completion);

        // Insert either task or precondition.
        let output = if self.as_precondition {
            Output::precondition(initializer)
        } else {
            Output::initializer(initializer)
        };
        Ok(output)
    }
}

#[async_trait::async_trait]
impl Task for NodeStorageInitializer {
    fn kind(&self) -> TaskKind {
        if self.stop_node_on_completion {
            TaskKind::UnconstrainedTask
        } else {
            TaskKind::UnconstrainedOneshotTask
        }
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

/// Runs [`NodeStorageInitializer`] as a precondition, blocking
/// tasks from starting until the storage is initialized.
#[derive(Debug)]
pub struct NodeStorageInitializerPrecondition(NodeStorageInitializer);

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
        try_stoppable!(self.0.wait_for_initialized_storage(stop_receiver.0).await);
        tracing::info!("Node storage initialization precondition completed");
        Ok(())
    }
}

impl Resource for NodeInitializationStrategy {
    fn name() -> String {
        "node_initialization_strategy".into()
    }
}
