use zksync_config::configs::chain::StateKeeperConfig;

use crate::{millis_since_epoch, UpdatesManager};

/// I/O-dependent seal criteria.
pub trait IoSealCriterion {
    fn should_seal_block(&mut self, manager: &UpdatesManager) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct TimeoutSealer {
    block_commit_deadline_ms: u64,
}

impl TimeoutSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            block_commit_deadline_ms: config.block_commit_deadline_ms,
        }
    }
}

impl IoSealCriterion for TimeoutSealer {
    fn should_seal_block(&mut self, manager: &UpdatesManager) -> bool {
        !manager.executed_transactions.is_empty()
            && millis_since(manager.timestamp_ms) > u128::from(self.block_commit_deadline_ms)
    }
}

fn millis_since(since: u128) -> u128 {
    millis_since_epoch() - since
}
