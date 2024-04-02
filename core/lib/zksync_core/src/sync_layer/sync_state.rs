use std::{sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::watch;
use zksync_concurrency::{ctx, sync};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_types::MiniblockNumber;
use zksync_web3_decl::{client::L2Client, namespaces::EthNamespaceClient};

use crate::{
    metrics::EN_METRICS,
    state_keeper::{io::IoCursor, updates::UpdatesManager, StateKeeperOutputHandler},
};

/// `SyncState` is a structure that holds the state of the syncing process.
/// The intended use case is to signalize to Web3 API whether the node is fully synced.
/// Data inside is expected to be updated by both `MainNodeFetcher` (on last block available on the main node)
/// and `ExternalIO` (on latest sealed miniblock).
///
/// This structure operates on miniblocks rather than L1 batches, since this is the default unit used in the web3 API.
#[derive(Debug, Clone)]
pub struct SyncState(Arc<sync::watch::Sender<SyncStateInner>>);

impl Default for SyncState {
    fn default() -> Self {
        Self(Arc::new(sync::watch::channel(SyncStateInner::default()).0))
    }
}

/// A threshold constant intended to keep the sync status less flaky.
/// This gives the external node some room to fetch new miniblocks without losing the sync status.
const SYNC_MINIBLOCK_DELTA: u32 = 10;

impl SyncState {
    pub(crate) fn get_main_node_block(&self) -> MiniblockNumber {
        self.0.borrow().main_node_block.unwrap_or_default()
    }

    pub(crate) fn get_local_block(&self) -> MiniblockNumber {
        self.0.borrow().local_block.unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) async fn wait_for_local_block(&self, want: MiniblockNumber) {
        self.0
            .subscribe()
            .wait_for(|inner| matches!(inner.local_block, Some(got) if got >= want))
            .await
            .unwrap();
    }

    pub(crate) async fn wait_for_main_node_block(
        &self,
        ctx: &ctx::Ctx,
        want: MiniblockNumber,
    ) -> ctx::OrCanceled<()> {
        sync::wait_for(
            ctx,
            &mut self.0.subscribe(),
            |inner| matches!(inner.main_node_block, Some(got) if got >= want),
        )
        .await?;
        Ok(())
    }

    pub(crate) fn set_main_node_block(&self, block: MiniblockNumber) {
        self.0.send_modify(|inner| inner.set_main_node_block(block));
    }

    fn set_local_block(&self, block: MiniblockNumber) {
        self.0.send_modify(|inner| inner.set_local_block(block));
    }

    pub(crate) fn is_synced(&self) -> bool {
        self.0.borrow().is_synced().0
    }

    pub async fn run_updater(
        self,
        connection_pool: ConnectionPool<Core>,
        main_node_client: L2Client,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        const UPDATE_INTERVAL: Duration = Duration::from_secs(10);

        while !*stop_receiver.borrow_and_update() {
            let local_block = connection_pool
                .connection()
                .await
                .context("Failed to get a connection from the pool in sync state updater")?
                .blocks_dal()
                .get_sealed_miniblock_number()
                .await
                .context("Failed to get the miniblock number from DB")?;

            let main_node_block = main_node_client
                .get_block_number()
                .await
                .context("Failed to request last miniblock number from main node")?;

            if let Some(local_block) = local_block {
                self.set_local_block(local_block);
                self.set_main_node_block(main_node_block.as_u32().into());
            }

            tokio::time::timeout(UPDATE_INTERVAL, stop_receiver.changed())
                .await
                .ok();
        }
        Ok(())
    }
}

#[async_trait]
impl StateKeeperOutputHandler for SyncState {
    async fn initialize(&mut self, cursor: &IoCursor) -> anyhow::Result<()> {
        let sealed_block_number = cursor.next_miniblock.saturating_sub(1);
        self.set_local_block(MiniblockNumber(sealed_block_number));
        Ok(())
    }

    async fn handle_miniblock(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        let sealed_block_number = updates_manager.miniblock.number;
        self.set_local_block(sealed_block_number);
        Ok(())
    }

    async fn handle_l1_batch(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        let sealed_block_number = updates_manager.miniblock.number;
        self.set_local_block(sealed_block_number);
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SyncStateInner {
    pub(crate) main_node_block: Option<MiniblockNumber>,
    pub(crate) local_block: Option<MiniblockNumber>,
}

impl SyncStateInner {
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
