use std::sync::Arc;

use zksync_core::state_keeper::{
    seal_criteria::ConditionalSealer, L1BatchExecutorBuilder, StateKeeperIO,
};

use crate::resource::{Resource, ResourceId, Unique};

#[derive(Debug, Clone)]
pub struct StateKeeperIOResource(pub Unique<Box<dyn StateKeeperIO>>);

impl Resource for StateKeeperIOResource {
    fn resource_id() -> ResourceId {
        "state_keeper/io".into()
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchExecutorBuilderResource(pub Unique<Box<dyn L1BatchExecutorBuilder>>);

impl Resource for L1BatchExecutorBuilderResource {
    fn resource_id() -> ResourceId {
        "state_keeper/l1_batch_executor_builder".into()
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalSealerResource(pub Arc<dyn ConditionalSealer>);

impl Resource for ConditionalSealerResource {
    fn resource_id() -> ResourceId {
        "state_keeper/conditional_sealer".into()
    }
}
