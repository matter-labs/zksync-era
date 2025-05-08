//! Shared dependency injection code for ZKsync node.

use zksync_node_framework::Resource;
use zksync_types::pubdata_da::PubdataSendingMode;

pub mod api;
pub mod contracts;

#[derive(Debug, Clone, Copy)]
pub struct PubdataSendingModeResource(pub PubdataSendingMode);

impl Resource for PubdataSendingModeResource {
    fn name() -> String {
        "common/pubdata_sending_mode".into()
    }
}
