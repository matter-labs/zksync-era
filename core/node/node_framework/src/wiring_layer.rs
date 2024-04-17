use std::fmt;

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

impl fmt::Debug for dyn WiringLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WiringLayer")
            .field("layer_name", &self.layer_name())
            .finish()
    }
}

/// An error that can occur during the wiring phase.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum WiringError {
    #[error("Layer attempted to add resource {name}, but it is already provided")]
    ResourceAlreadyProvided { id: ResourceId, name: String },
    #[error("Resource {name} is not provided")]
    ResourceLacking { id: ResourceId, name: String },
    #[error("Wiring layer has been incorrectly configured: {0}")]
    Configuration(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
