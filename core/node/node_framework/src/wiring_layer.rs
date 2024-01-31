use crate::{node::NodeContext, resource::ResourceId};

#[async_trait::async_trait]
pub trait WiringLayer: 'static + Send + Sync {
    /// Identifier of the wiring layer.
    fn layer_name(&self) -> &'static str;

    async fn wire(self: Box<Self>, node: NodeContext<'_>) -> Result<(), WiringError>;
}

/// An error that can occur during the wiring phase.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum WiringError {
    #[error("Resource {0} is not provided")]
    ResourceLacking(ResourceId),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
