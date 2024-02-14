use crate::{resource::ResourceId, service::ServiceContext};

/// Wiring layer provides a way to customize the `ZkStackService` by
/// adding new tasks or resources to it.
///
/// Structures that implement this trait are advised to specify in doc comments
/// which resources they use or add, and the list of tasks they add.
#[async_trait::async_trait]
pub trait WiringLayer: 'static + Send + Sync {
    /// Identifier of the wiring layer.
    fn layer_name(&self) -> &'static str;

    /// Performs the wiring process, e.g. adds tasks and resources to the node.
    /// This method will be called once during the node initialization.
    async fn wire(self: Box<Self>, context: ServiceContext<'_>) -> Result<(), WiringError>;
}

/// An error that can occur during the wiring phase.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum WiringError {
    #[error("Layer attempted to add resource {0}, but it is already provided")]
    ResourceAlreadyProvided(ResourceId),
    #[error("Resource {0} is not provided")]
    ResourceLacking(ResourceId),
    #[error("Wiring layer has been incorrectly configured: {0}")]
    Configuration(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
