use serde::{Deserialize, Serialize};
use zksync_health_check::{Health, HealthStatus};
use zksync_types::{L1BatchNumber, L2BlockNumber, H256};

use crate::io::IoCursor;

#[derive(Debug, Serialize, Deserialize)]
pub struct StateKeeperHealthDetails {
    pub next_l2_block: L2BlockNumber,
    pub prev_l2_block_hash: H256,
    pub prev_l2_block_timestamp: u64,
    pub l1_batch: L1BatchNumber,
}

impl From<StateKeeperHealthDetails> for Health {
    fn from(details: StateKeeperHealthDetails) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

impl From<&IoCursor> for StateKeeperHealthDetails {
    fn from(details: &IoCursor) -> Self {
        Self {
            next_l2_block: details.next_l2_block,
            prev_l2_block_hash: details.prev_l2_block_hash,
            prev_l2_block_timestamp: details.prev_l2_block_timestamp,
            l1_batch: details.l1_batch,
        }
    }
}
