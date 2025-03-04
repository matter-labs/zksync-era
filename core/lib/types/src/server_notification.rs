use zksync_basic_types::L1BlockNumber;

use crate::H256;

#[derive(Debug, PartialEq)]
pub enum GatewayMigrationState {
    Not,
    Started,
    Finalized,
}

#[derive(Debug, Clone)]
pub struct ServerNotification {
    pub l1_block_number: L1BlockNumber,
    pub main_topic: H256,
    pub value: Option<serde_json::Value>,
}
