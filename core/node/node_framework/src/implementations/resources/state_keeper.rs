use std::sync::Arc;

use zksync_state_keeper::{
    seal_criteria::ConditionalSealer, BatchExecutor, OutputHandler, StateKeeperIO,
};

use crate::resource::{Resource, Unique};

#[derive(Debug, Clone)]
pub struct StateKeeperIOResource(pub Unique<Box<dyn StateKeeperIO>>);

impl Resource for StateKeeperIOResource {
    fn name() -> String {
        "state_keeper/io".into()
    }
}

#[derive(Debug, Clone)]
pub struct BatchExecutorResource(pub Unique<Box<dyn BatchExecutor>>);

impl Resource for BatchExecutorResource {
    fn name() -> String {
        "state_keeper/batch_executor".into()
    }
}

#[derive(Debug, Clone)]
pub struct OutputHandlerResource(pub Unique<OutputHandler>);

impl Resource for OutputHandlerResource {
    fn name() -> String {
        "state_keeper/output_handler".into()
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalSealerResource(pub Arc<dyn ConditionalSealer>);

impl Resource for ConditionalSealerResource {
    fn name() -> String {
        "state_keeper/conditional_sealer".into()
    }
}
