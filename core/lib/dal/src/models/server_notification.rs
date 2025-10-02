use zksync_types::{L1BlockNumber, H256};

pub struct ServerNotification {
    pub l1_block_number: L1BlockNumber,
    pub main_topic: H256,
}
