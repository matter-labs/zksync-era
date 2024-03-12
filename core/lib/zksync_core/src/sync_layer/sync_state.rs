use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::Serialize;
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_types::MiniblockNumber;

use crate::metrics::EN_METRICS;

/// `SyncState` is a structure that holds the state of the syncing process.
/// The intended use case is to signalize to Web3 API whether the node is fully synced.
/// Data inside is expected to be updated by both `MainNodeFetcher` (on last block available on the main node)
/// and `ExternalIO` (on latest sealed miniblock).
///
/// This structure operates on miniblocks rather than L1 batches, since this is the default unit used in the web3 API.
#[derive(Debug, Default, Clone)]
pub struct SyncState {
    inner: Arc<RwLock<SyncStateInner>>,
}

/// A threshold constant intended to keep the sync status less flaky.
/// This gives the external node some room to fetch new miniblocks without losing the sync status.
const SYNC_MINIBLOCK_DELTA: u32 = 10;

impl SyncState {
    pub(crate) fn get_main_node_block(&self) -> MiniblockNumber {
        self.inner
            .read()
            .unwrap()
            .main_node_block
            .unwrap_or_default()
    }

    pub(crate) fn get_local_block(&self) -> MiniblockNumber {
        self.inner.read().unwrap().local_block.unwrap_or_default()
    }

    pub(crate) fn set_main_node_block(&self, block: MiniblockNumber) {
        let mut inner = self.inner.write().unwrap();
        if let Some(local_block) = inner.local_block {
            tracing::info!("current local block: {}", local_block.0);
            tracing::info!("current main node block: {}", block.0);
            if block.0 < local_block.0 {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!(
                    "main_node_block({}) is less than local_block({})",
                    block,
                    local_block
                );
            }
        }
        inner.main_node_block = Some(block);
        inner.update_sync_metric();
    }

    pub(super) fn set_local_block(&self, block: MiniblockNumber) {
        let mut inner = self.inner.write().unwrap();
        if let Some(main_node_block) = inner.main_node_block {
            if block.0 > main_node_block.0 {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!(
                    "local_block({}) is greater than main_node_block({})",
                    block,
                    main_node_block
                );
            }
        }
        inner.local_block = Some(block);
        inner.update_sync_metric();
    }

    pub(crate) fn is_synced(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.is_synced().0
    }
}

#[async_trait]
impl CheckHealth for SyncState {
    fn name(&self) -> &'static str {
        "sync_state"
    }

    async fn check_health(&self) -> Health {
        Health::from(&*self.inner.read().unwrap())
    }
}

#[derive(Debug, Default)]
struct SyncStateInner {
    main_node_block: Option<MiniblockNumber>,
    local_block: Option<MiniblockNumber>,
}

impl SyncStateInner {
    fn is_synced(&self) -> (bool, Option<u32>) {
        if let (Some(main_node_block), Some(local_block)) = (self.main_node_block, self.local_block)
        {
            let Some(block_diff) = main_node_block.0.checked_sub(local_block.0) else {
                // We're ahead of the main node, this situation is handled by the re-org detector.
                return (true, Some(0));
            };
            (block_diff <= SYNC_MINIBLOCK_DELTA, Some(block_diff))
        } else {
            (false, None)
        }
    }

    fn update_sync_metric(&self) {
        let (is_synced, lag) = self.is_synced();
        EN_METRICS.synced.set(is_synced.into());
        if let Some(lag) = lag {
            EN_METRICS.sync_lag.set(lag.into());
        }
    }
}

impl From<&SyncStateInner> for Health {
    fn from(state: &SyncStateInner) -> Health {
        #[derive(Debug, Serialize)]
        struct SyncStateHealthDetails {
            is_synced: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            main_node_block: Option<MiniblockNumber>,
            #[serde(skip_serializing_if = "Option::is_none")]
            local_block: Option<MiniblockNumber>,
        }

        let (is_synced, block_diff) = state.is_synced();
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
        assert!(!sync_state.is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::NotReady);

        // The gap is too big, still not synced.
        sync_state.set_local_block(MiniblockNumber(0));
        sync_state.set_main_node_block(MiniblockNumber(SYNC_MINIBLOCK_DELTA + 1));
        assert!(!sync_state.is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::Affected);

        // Within the threshold, the node is synced.
        sync_state.set_local_block(MiniblockNumber(1));
        assert!(sync_state.is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::Ready);

        // Can reach the main node last block.
        sync_state.set_local_block(MiniblockNumber(SYNC_MINIBLOCK_DELTA + 1));
        assert!(sync_state.is_synced());

        // Main node can again move forward.
        sync_state.set_main_node_block(MiniblockNumber(2 * SYNC_MINIBLOCK_DELTA + 2));
        assert!(!sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_local_block() {
        let sync_state = SyncState::default();

        sync_state.set_main_node_block(MiniblockNumber(1));
        sync_state.set_local_block(MiniblockNumber(2));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_main_node_block() {
        let sync_state = SyncState::default();

        sync_state.set_local_block(MiniblockNumber(2));
        sync_state.set_main_node_block(MiniblockNumber(1));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }
}
