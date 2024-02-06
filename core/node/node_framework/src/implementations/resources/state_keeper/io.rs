use std::sync::Arc;

use zksync_core::state_keeper::StateKeeperIO;

use crate::resource::{Resource, ResourceId, Unique};

/// Wrapper for the object store.
#[derive(Debug, Clone)]
pub struct StateKeeperIOResource(pub Unique<Box<dyn StateKeeperIO>>);

impl Resource for StateKeeperIOResource {
    fn resource_id() -> ResourceId {
        "state_keeper/io".into()
    }
}
