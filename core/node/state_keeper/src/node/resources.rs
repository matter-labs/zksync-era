use zksync_node_framework::{
    resource,
    resource::{Resource, Unique},
};
use zksync_state::OwnedStorage;
use zksync_vm_executor::interface::BatchExecutorFactory;

use crate::{
    seal_criteria::ConditionalSealer, OutputHandler, StateKeeper, StateKeeperIO, StateKeeperInner,
};

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

/// A resource that provides [`BatchExecutorFactory`] implementation to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct BatchExecutorResource(pub Unique<Box<dyn BatchExecutorFactory<OwnedStorage>>>);

impl Resource for BatchExecutorResource {
    fn name() -> String {
        "state_keeper/batch_executor".into()
    }
}

impl<T> From<T> for BatchExecutorResource
where
    T: BatchExecutorFactory<OwnedStorage>,
{
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

impl Resource<resource::Shared> for dyn ConditionalSealer {
    fn name() -> String {
        "state_keeper/conditional_sealer".into()
    }
}

#[derive(Debug, Clone)]
pub struct StateKeeperResource(pub Unique<StateKeeperInner>);

impl From<StateKeeperInner> for StateKeeperResource {
    fn from(sk: StateKeeperInner) -> Self {
        Self(Unique::new(sk))
    }
}

impl Resource for StateKeeperResource {
    fn name() -> String {
        "state_keeper/state_keeper".into()
    }
}
