use std::sync::Arc;

use zksync_state::OwnedStorage;
use zksync_state_keeper::{seal_criteria::ConditionalSealer, OutputHandler, StateKeeperIO};
use zksync_vm_executor::interface::BatchExecutorFactory;
use zksync_zkos_state_keeper::{
    ConditionalSealer as ZkOsConditionalSealer, OutputHandler as ZkOsOutputHandler,
    StateKeeperIO as ZkOsStateKeeperIO,
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

/// A resource that provides [`ZkOsStateKeeperIO`] implementation to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct ZkOsStateKeeperIOResource(pub Unique<Box<dyn ZkOsStateKeeperIO>>);

impl Resource for ZkOsStateKeeperIOResource {
    fn name() -> String {
        "zk_os_state_keeper/io".into()
    }
}

impl<T: ZkOsStateKeeperIO> From<T> for ZkOsStateKeeperIOResource {
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

/// A resource that provides [`ZkOsOutputHandler`] implementation to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct ZkOsOutputHandlerResource(pub Unique<ZkOsOutputHandler>);

impl Resource for ZkOsOutputHandlerResource {
    fn name() -> String {
        "state_keeper/zk_os_output_handler".into()
    }
}

impl From<ZkOsOutputHandler> for ZkOsOutputHandlerResource {
    fn from(handler: ZkOsOutputHandler) -> Self {
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

/// A resource that provides [`ZkOsConditionalSealer`] implementation to the service.
#[derive(Debug, Clone)]
pub struct ZkOsConditionalSealerResource(pub Arc<dyn ZkOsConditionalSealer>);

impl Resource for ZkOsConditionalSealerResource {
    fn name() -> String {
        "state_keeper/zk_os_conditional_sealer".into()
    }
}

impl<T> From<T> for ZkOsConditionalSealerResource
where
    T: ZkOsConditionalSealer + 'static,
{
    fn from(sealer: T) -> Self {
        Self(Arc::new(sealer))
    }
}
