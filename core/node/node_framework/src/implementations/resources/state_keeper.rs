use std::sync::Arc;

use zksync_core::state_keeper::{seal_criteria::ConditionalSealer, BatchExecutor, StateKeeperIO};

use crate::resource::{Resource, ResourceId, Unique};

#[derive(Debug, Clone)]
pub struct StateKeeperIOResource(pub Unique<Box<dyn StateKeeperIO>>);

impl Resource for StateKeeperIOResource {
    fn resource_id() -> ResourceId {
        "state_keeper/io".into()
    }
}

#[derive(Debug, Clone)]
pub struct BatchExecutorResource(pub Unique<Box<dyn BatchExecutor>>);

impl Resource for BatchExecutorResource {
    fn resource_id() -> ResourceId {
        "state_keeper/batch_executor".into()
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalSealerResource(pub Arc<dyn ConditionalSealer>);

impl Resource for ConditionalSealerResource {
    fn resource_id() -> ResourceId {
        "state_keeper/conditional_sealer".into()
    }
}
