//! Shared dependency injection code for ZKsync node.

use zksync_basic_types::pubdata_da::PubdataSendingMode;
use zksync_node_framework::Resource;

pub mod contracts;

#[derive(Debug, Clone, Copy)]
pub struct PubdataSendingModeResource(pub PubdataSendingMode);

impl Resource for PubdataSendingModeResource {
    fn name() -> String {
        "common/pubdata_sending_mode".into()
    }
}
