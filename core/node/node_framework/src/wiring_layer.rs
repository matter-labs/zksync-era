use std::fmt;

use tokio::runtime;

use crate::{resource::ResourceId, service::ServiceContext, FromContext, IntoContext};

/// An envelope for the wiring layer function.
/// Since `WiringLayer` has associated types, we cannot easily erase the types via `dyn WiringLayer`,
/// so instead we preserve the layer type within the closure, and represent the actual wiring logic
/// as a function of the service context instead.
/// See [`WiringLayerExt`] trait for more context.
#[allow(clippy::type_complexity)] // False positive, already a dedicated type.
pub(crate) struct WireFn(
    pub Box<dyn FnOnce(&runtime::Handle, &mut ServiceContext<'_>) -> Result<(), WiringError>>,
);

impl fmt::Debug for WireFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WireFn").finish()
    }
}

/// Wiring layer provides a way to customize the `ZkStackService` by
/// adding new tasks or resources to it.
///
/// Structures that implement this trait are advised to specify in doc comments
/// which resources they use or add, and the list of tasks they add.
#[async_trait::async_trait]
pub trait WiringLayer: 'static + Send + Sync {
    type Input: FromContext;
    type Output: IntoContext;

    /// Identifier of the wiring layer.
    fn layer_name(&self) -> &'static str;

    /// Performs the wiring process, e.g. adds tasks and resources to the node.
    /// This method will be called once during the node initialization.
    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError>;
}

pub(crate) trait WiringLayerExt: WiringLayer {
    /// Hires the actual type of the wiring layer into the closure, so that rest of application
    /// doesn't have to know it.
    fn into_wire_fn(self) -> WireFn
    where
        Self: Sized,
    {
        WireFn(Box::new(move |rt, ctx| {
            let input = Self::Input::from_context(ctx)?;
            let output = rt.block_on(self.wire(input))?;
            output.into_context(ctx)?;
            Ok(())
        }))
    }
}

impl<T> WiringLayerExt for T where T: WiringLayer {}

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

impl WiringError {
    /// Wraps the specified internal error.
    pub fn internal(err: impl Into<anyhow::Error>) -> Self {
        Self::Internal(err.into())
    }
}
