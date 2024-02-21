use std::sync::Arc;

use tokio::sync::watch;
use zksync_types::MiniblockNumber;

use crate::metrics::EN_METRICS;

/// `SyncState` is a structure that holds the state of the syncing process.
/// The intended use case is to signalize to Web3 API whether the node is fully synced.
/// Data inside is expected to be updated by both `MainNodeFetcher` (on last block available on the main node)
/// and `ExternalIO` (on latest sealed miniblock).
///
/// This structure operates on miniblocks rather than L1 batches, since this is the default unit used in the web3 API.
#[derive(Debug, Clone)]
pub struct SyncState(Arc<watch::Sender<SyncStateInner>>);

impl Default for SyncState {
    fn default() -> Self {
        Self(Arc::new(watch::channel(SyncStateInner::default()).0))
    }
}

/// A threshold constant intended to keep the sync status less flaky.
/// This gives the external node some room to fetch new miniblocks without losing the sync status.
const SYNC_MINIBLOCK_DELTA: u32 = 10;

impl SyncState {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get_main_node_block(&self) -> MiniblockNumber {
        self.0.borrow().main_node_block()
    }

    pub(crate) fn get_local_block(&self) -> MiniblockNumber {
        self.0.borrow().local_block()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<SyncStateInner> {
        self.0.subscribe()
    }

    pub(crate) fn set_main_node_block(&self, block: MiniblockNumber) {
        self.0.send_modify(|inner| inner.set_main_node_block(block));
    }

    pub(super) fn set_local_block(&self, block: MiniblockNumber) {
        self.0.send_modify(|inner| inner.set_local_block(block));
    }

    pub(crate) fn is_synced(&self) -> bool {
        self.0.borrow().is_synced().0
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SyncStateInner {
    main_node_block: Option<MiniblockNumber>,
    local_block: Option<MiniblockNumber>,
}

impl SyncStateInner {
    pub(crate) fn main_node_block(&self) -> MiniblockNumber {
        self.main_node_block.unwrap_or_default()
    }

    pub(crate) fn local_block(&self) -> MiniblockNumber {
        self.local_block.unwrap_or_default()
    }

    fn set_main_node_block(&mut self, block: MiniblockNumber) {
        if let Some(local_block) = self.local_block {
            if block < local_block {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!("main_node_block({block}) is less than local_block({local_block})");
            }
        }
        self.main_node_block = Some(block);
        self.update_sync_metric();
    }

    fn set_local_block(&mut self, block: MiniblockNumber) {
        if let Some(main_node_block) = self.main_node_block {
            if block > main_node_block {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!(
                    "local_block({block}) is greater than main_node_block({main_node_block})"
                );
            }
        }
        self.local_block = Some(block);
        self.update_sync_metric();
    }

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
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_main_node_block() {
        let sync_state = SyncState::new();

        sync_state.set_local_block(MiniblockNumber(2));
        sync_state.set_main_node_block(MiniblockNumber(1));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }
}
