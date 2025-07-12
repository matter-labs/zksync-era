use std::{ops, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_node_framework::Resource;
use zksync_types::L2BlockNumber;

/// `SyncState` is a structure that holds the state of the syncing process.
///
/// The intended use case is to signalize to Web3 API whether the node is fully synced.
/// Data inside is expected to be updated by both `MainNodeFetcher` (on last block available on the main node)
/// and `ExternalIO` (on latest sealed L2 block).
///
/// This structure operates on L2 blocks rather than L1 batches, since this is the default unit used in the web3 API.
#[derive(Debug, Clone)]
pub struct SyncState(Arc<watch::Sender<SyncStateData>>);

impl Default for SyncState {
    fn default() -> Self {
        Self(Arc::new(watch::channel(SyncStateData::default()).0))
    }
}

impl Resource for SyncState {
    fn name() -> String {
        "common/sync_state".into()
    }
}

/// A threshold constant intended to keep the sync status less flaky.
/// This gives the external node some room to fetch new L2 blocks without losing the sync status.
const SYNC_L2_BLOCK_DELTA: u32 = 10;

impl SyncState {
    pub fn set_main_node_block(&self, block: L2BlockNumber) {
        self.0.send_modify(|inner| inner.set_main_node_block(block));
    }

    pub fn set_local_block(&self, block: L2BlockNumber) {
        self.0.send_modify(|inner| inner.set_local_block(block));
    }

    pub fn borrow(&self) -> impl ops::Deref<Target = SyncStateData> + '_ {
        self.0.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<SyncStateData> {
        self.0.subscribe()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncStateData {
    main_node_block: Option<L2BlockNumber>,
    local_block: Option<L2BlockNumber>,
}

impl SyncStateData {
    pub fn lag(&self) -> (bool, Option<u32>) {
        if let (Some(main_node_block), Some(local_block)) = (self.main_node_block, self.local_block)
        {
            let Some(block_diff) = main_node_block.0.checked_sub(local_block.0) else {
                // We're ahead of the main node, this situation is handled by the re-org detector.
                return (true, Some(0));
            };
            (block_diff <= SYNC_L2_BLOCK_DELTA, Some(block_diff))
        } else {
            (false, None)
        }
    }

    pub fn is_synced(&self) -> bool {
        self.lag().0
    }

    pub fn local_block(&self) -> Option<L2BlockNumber> {
        self.local_block
    }

    pub fn main_node_block(&self) -> Option<L2BlockNumber> {
        self.main_node_block
    }

    fn set_main_node_block(&mut self, block: L2BlockNumber) {
        if let Some(local_block) = self.local_block {
            if block < local_block {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!("main_node_block({block}) is less than local_block({local_block})");
            }
        }
        self.main_node_block = Some(block);
    }

    fn set_local_block(&mut self, block: L2BlockNumber) {
        if let Some(main_node_block) = self.main_node_block {
            if block > main_node_block {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!(
                    "local_block({block}) is greater than main_node_block({main_node_block})"
                );
            }
        }
        self.local_block = Some(block);
    }
}

#[async_trait]
impl CheckHealth for SyncState {
    fn name(&self) -> &'static str {
        "sync_state"
    }

    async fn check_health(&self) -> Health {
        Health::from(&*self.0.borrow())
    }
}

impl From<&SyncStateData> for Health {
    fn from(state: &SyncStateData) -> Health {
        #[derive(Debug, Serialize)]
        struct SyncStateHealthDetails {
            is_synced: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            main_node_block: Option<L2BlockNumber>,
            #[serde(skip_serializing_if = "Option::is_none")]
            local_block: Option<L2BlockNumber>,
        }

        let (is_synced, block_diff) = state.lag();
        let status = if is_synced {
            HealthStatus::Ready
        } else if block_diff.is_some() {
            HealthStatus::Affected
        } else {
            return HealthStatus::NotReady.into(); // `state` isn't initialized yet
        };
        Health::from(status).with_details(SyncStateHealthDetails {
            is_synced,
            main_node_block: state.main_node_block,
            local_block: state.local_block,
        })
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[tokio::test]
    async fn test_sync_state() {
        let sync_state = SyncState::default();

        // The node is not synced if there is no data.
        assert!(!sync_state.borrow().is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::NotReady);

        // The gap is too big, still not synced.
        sync_state.set_local_block(L2BlockNumber(0));
        sync_state.set_main_node_block(L2BlockNumber(SYNC_L2_BLOCK_DELTA + 1));
        assert!(!sync_state.borrow().is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::Affected);

        // Within the threshold, the node is synced.
        sync_state.set_local_block(L2BlockNumber(1));
        assert!(sync_state.borrow().is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::Ready);

        // Can reach the main node last block.
        sync_state.set_local_block(L2BlockNumber(SYNC_L2_BLOCK_DELTA + 1));
        assert!(sync_state.borrow().is_synced());

        // Main node can again move forward.
        sync_state.set_main_node_block(L2BlockNumber(2 * SYNC_L2_BLOCK_DELTA + 2));
        assert!(!sync_state.borrow().is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_local_block() {
        let sync_state = SyncState::default();

        sync_state.set_main_node_block(L2BlockNumber(1));
        sync_state.set_local_block(L2BlockNumber(2));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.borrow().is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_main_node_block() {
        let sync_state = SyncState::default();

        sync_state.set_local_block(L2BlockNumber(2));
        sync_state.set_main_node_block(L2BlockNumber(1));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.borrow().is_synced());
    }
}
