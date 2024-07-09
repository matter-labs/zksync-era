use zksync_node_storage_init::{NodeInitializationStrategy, NodeStorageInitializer};

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    resource::Resource,
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

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

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub strategy: NodeInitializationStrategyResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub initializer: Option<NodeStorageInitializer>,
    #[context(task)]
    pub precondition: Option<NodeStorageInitializerPrecondition>,
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
        let NodeInitializationStrategyResource(strategy) = input.strategy;

        let initializer = NodeStorageInitializer::new(strategy, pool);

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
        let result = self.0.wait_for_initialized_storage(stop_receiver.0).await;
        tracing::info!("Node storage initialization precondition completed");
        result
    }
}

// Note: unlike with other modules, this one keeps within the same file to simplify
// moving the implementations out of the framework soon.
/// Resource representing the node initialization strategy.
#[derive(Debug, Clone)]
pub struct NodeInitializationStrategyResource(NodeInitializationStrategy);

impl Resource for NodeInitializationStrategyResource {
    fn name() -> String {
        "node_initialization_strategy".into()
    }
}

impl From<NodeInitializationStrategy> for NodeInitializationStrategyResource {
    fn from(strategy: NodeInitializationStrategy) -> Self {
        Self(strategy)
    }
}
