use std::sync::Arc;

use zksync_state_keeper::{
    seal_criteria::ConditionalSealer, BatchExecutor, OutputHandler, StateKeeperIO,
};

use crate::resource::{Resource, Unique};

/// A resource that provides [`StateKeeperIO`] implementation to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct StateKeeperIOResource(pub Unique<Box<dyn StateKeeperIO>>);

impl Resource for StateKeeperIOResource {
    fn name() -> String {
        "state_keeper/io".into()
    }
}

impl<T: StateKeeperIO> From<T> for StateKeeperIOResource {
    fn from(io: T) -> Self {
        Self(Unique::new(Box::new(io)))
    }
}

/// A resource that provides [`BatchExecutor`] implementation to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct BatchExecutorResource(pub Unique<Box<dyn BatchExecutor>>);

impl Resource for BatchExecutorResource {
    fn name() -> String {
        "state_keeper/batch_executor".into()
    }
}

impl<T: BatchExecutor> From<T> for BatchExecutorResource {
    fn from(executor: T) -> Self {
        Self(Unique::new(Box::new(executor)))
    }
}

/// A resource that provides [`OutputHandler`] implementation to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct OutputHandlerResource(pub Unique<OutputHandler>);

impl Resource for OutputHandlerResource {
    fn name() -> String {
        "state_keeper/output_handler".into()
    }
}

impl From<OutputHandler> for OutputHandlerResource {
    fn from(handler: OutputHandler) -> Self {
        Self(Unique::new(handler))
    }
}

/// A resource that provides [`ConditionalSealer`] implementation to the service.
#[derive(Debug, Clone)]
pub struct ConditionalSealerResource(pub Arc<dyn ConditionalSealer>);

impl Resource for ConditionalSealerResource {
    fn name() -> String {
        "state_keeper/conditional_sealer".into()
    }
}

impl<T> From<T> for ConditionalSealerResource
where
    T: ConditionalSealer + 'static,
{
    fn from(sealer: T) -> Self {
        Self(Arc::new(sealer))
    }
}
