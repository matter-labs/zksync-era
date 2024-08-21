use std::sync::Arc;

use zksync_state::OwnedStorage;
use zksync_state_keeper::{seal_criteria::ConditionalSealer, OutputHandler, StateKeeperIO};
use zksync_vm_utils::interface::{box_batch_executor, BatchExecutor, BoxBatchExecutor, Standard};

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
pub struct BatchExecutorResource(pub Unique<BoxBatchExecutor<OwnedStorage>>);

impl Resource for BatchExecutorResource {
    fn name() -> String {
        "state_keeper/batch_executor".into()
    }
}

impl<T> From<T> for BatchExecutorResource
where
    T: BatchExecutor<OwnedStorage, Outputs = Standard<OwnedStorage>, Handle: Sized>,
{
    fn from(executor: T) -> Self {
        Self(Unique::new(box_batch_executor(executor)))
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
