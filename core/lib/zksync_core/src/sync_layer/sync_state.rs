use std::sync::{Arc, RwLock};

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
    pub fn new() -> Self {
        Self::default()
    }

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

    pub(super) fn set_main_node_block(&self, block: MiniblockNumber) {
        let mut inner = self.inner.write().unwrap();
        if let Some(local_block) = inner.local_block {
            if block.0 < local_block.0 {
                // Probably it's fine -- will be checked by the reorg detector.
                tracing::warn!(
                    "main_node_block({}) is less than local_block({})",
                    block,
                    local_block
                );
            }
        }
        inner.main_node_block = Some(block);
        self.update_sync_metric(&inner);
    }

    pub(super) fn set_local_block(&self, block: MiniblockNumber) {
        let mut inner = self.inner.write().unwrap();
        if let Some(main_node_block) = inner.main_node_block {
            if block.0 > main_node_block.0 {
                // Probably it's fine -- will be checked by the reorg detector.
                tracing::warn!(
                    "local_block({}) is greater than main_node_block({})",
                    block,
                    main_node_block
                );
            }
        }
        inner.local_block = Some(block);
        self.update_sync_metric(&inner);
    }

    pub(crate) fn is_synced(&self) -> bool {
        let inner = self.inner.read().unwrap();
        self.is_synced_inner(&inner).0
    }

    fn update_sync_metric(&self, inner: &SyncStateInner) {
        let (is_synced, lag) = self.is_synced_inner(inner);
        EN_METRICS.synced.set(is_synced.into());
        if let Some(lag) = lag {
            EN_METRICS.sync_lag.set(lag.into());
        }
    }

    fn is_synced_inner(&self, inner: &SyncStateInner) -> (bool, Option<u32>) {
        if let (Some(main_node_block), Some(local_block)) =
            (inner.main_node_block, inner.local_block)
        {
            let Some(block_diff) = main_node_block.0.checked_sub(local_block.0) else {
                // We're ahead of the main node, this situation is handled by the reorg detector.
                return (true, Some(0));
            };
            (block_diff <= SYNC_MINIBLOCK_DELTA, Some(block_diff))
        } else {
            (false, None)
        }
    }
}

#[derive(Debug, Default)]
struct SyncStateInner {
    main_node_block: Option<MiniblockNumber>,
    local_block: Option<MiniblockNumber>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_state() {
        let sync_state = SyncState::new();

        // The node is not synced if there is no data.
        assert!(!sync_state.is_synced());

        // The gap is too big, still not synced.
        sync_state.set_local_block(MiniblockNumber(0));
        sync_state.set_main_node_block(MiniblockNumber(SYNC_MINIBLOCK_DELTA + 1));
        assert!(!sync_state.is_synced());

        // Within the threshold, the node is synced.
        sync_state.set_local_block(MiniblockNumber(1));
        assert!(sync_state.is_synced());

        // Can reach the main node last block.
        sync_state.set_local_block(MiniblockNumber(SYNC_MINIBLOCK_DELTA + 1));
        assert!(sync_state.is_synced());

        // Main node can again move forward.
        sync_state.set_main_node_block(MiniblockNumber(2 * SYNC_MINIBLOCK_DELTA + 2));
        assert!(!sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_local_block() {
        let sync_state = SyncState::new();

        sync_state.set_main_node_block(MiniblockNumber(1));
        sync_state.set_local_block(MiniblockNumber(2));
        // ^ should not panic, as we defer the situation to the reorg detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_main_node_block() {
        let sync_state = SyncState::new();

        sync_state.set_local_block(MiniblockNumber(2));
        sync_state.set_main_node_block(MiniblockNumber(1));
        // ^ should not panic, as we defer the situation to the reorg detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }
}
