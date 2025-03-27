use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::watch;
use zksync_concurrency::{ctx, sync};
use zksync_consensus_roles::validator;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_shared_metrics::EN_METRICS;
use zksync_state_keeper::{io::IoCursor, updates::UpdatesManager, StateKeeperOutputHandler};
use zksync_types::L2BlockNumber;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::EthNamespaceClient,
};

/// `SyncState` is a structure that holds the state of the syncing process.
///
/// The intended use case is to signalize to Web3 API whether the node is fully synced.
/// Data inside is expected to be updated by both `MainNodeFetcher` (on last block available on the main node)
/// and `ExternalIO` (on latest sealed L2 block).
///
/// This structure operates on L2 blocks rather than L1 batches, since this is the default unit used in the web3 API.
#[derive(Debug, Clone)]
pub struct SyncState(Arc<watch::Sender<SyncStateInner>>);

impl Default for SyncState {
    fn default() -> Self {
        Self(Arc::new(watch::channel(SyncStateInner::default()).0))
    }
}

/// A threshold constant intended to keep the sync status less flaky.
/// This gives the external node some room to fetch new L2 blocks without losing the sync status.
const SYNC_L2_BLOCK_DELTA: u32 = 10;

impl SyncState {
    pub fn get_main_node_block(&self) -> L2BlockNumber {
        self.0.borrow().main_node_block.unwrap_or_default()
    }

    pub fn get_local_block(&self) -> L2BlockNumber {
        self.0.borrow().local_block.unwrap_or_default()
    }

    pub async fn wait_for_local_block(&self, want: L2BlockNumber) {
        self.0
            .subscribe()
            .wait_for(|inner| matches!(inner.local_block, Some(got) if got >= want))
            .await
            .unwrap();
    }

    /// Waits until the main node block is greater or equal to the given block number.
    /// Returns the current main node block number.
    pub async fn wait_for_main_node_block(
        &self,
        ctx: &ctx::Ctx,
        pred: impl Fn(validator::BlockNumber) -> bool,
    ) -> ctx::OrCanceled<validator::BlockNumber> {
        sync::wait_for_some(ctx, &mut self.0.subscribe(), |inner| {
            inner
                .main_node_block
                .map(|n| validator::BlockNumber(n.0.into()))
                .filter(|n| pred(*n))
        })
        .await
    }

    pub fn set_main_node_block(&self, block: L2BlockNumber) {
        self.0.send_modify(|inner| inner.set_main_node_block(block));
    }

    fn set_local_block(&self, block: L2BlockNumber) {
        self.0.send_modify(|inner| inner.set_local_block(block));
    }

    pub fn is_synced(&self) -> bool {
        self.0.borrow().is_synced().0
    }

    pub async fn run_updater(
        self,
        connection_pool: ConnectionPool<Core>,
        main_node_client: Box<DynClient<L2>>,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        const UPDATE_INTERVAL: Duration = Duration::from_secs(10);

        while !*stop_receiver.borrow_and_update() {
            let local_block = connection_pool
                .connection()
                .await?
                .blocks_dal()
                .get_sealed_l2_block_number()
                .await?;

            let main_node_block = main_node_client.get_block_number().await?;

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
        let sealed_block_number = cursor.next_l2_block.saturating_sub(1);
        self.set_local_block(L2BlockNumber(sealed_block_number));
        Ok(())
    }

    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        let sealed_block_number = updates_manager.l2_block.number;
        self.set_local_block(sealed_block_number);
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let sealed_block_number = updates_manager.l2_block.number;
        self.set_local_block(sealed_block_number);
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SyncStateInner {
    pub(crate) main_node_block: Option<L2BlockNumber>,
    pub(crate) local_block: Option<L2BlockNumber>,
}

impl SyncStateInner {
    fn set_main_node_block(&mut self, block: L2BlockNumber) {
        if let Some(local_block) = self.local_block {
            if block < local_block {
                // Probably it's fine -- will be checked by the re-org detector.
                tracing::warn!("main_node_block({block}) is less than local_block({local_block})");
            }
        }
        self.main_node_block = Some(block);
        self.update_sync_metric();
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
            (block_diff <= SYNC_L2_BLOCK_DELTA, Some(block_diff))
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
            main_node_block: Option<L2BlockNumber>,
            #[serde(skip_serializing_if = "Option::is_none")]
            local_block: Option<L2BlockNumber>,
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
        sync_state.set_local_block(L2BlockNumber(0));
        sync_state.set_main_node_block(L2BlockNumber(SYNC_L2_BLOCK_DELTA + 1));
        assert!(!sync_state.is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::Affected);

        // Within the threshold, the node is synced.
        sync_state.set_local_block(L2BlockNumber(1));
        assert!(sync_state.is_synced());

        let health = sync_state.check_health().await;
        assert_matches!(health.status(), HealthStatus::Ready);

        // Can reach the main node last block.
        sync_state.set_local_block(L2BlockNumber(SYNC_L2_BLOCK_DELTA + 1));
        assert!(sync_state.is_synced());

        // Main node can again move forward.
        sync_state.set_main_node_block(L2BlockNumber(2 * SYNC_L2_BLOCK_DELTA + 2));
        assert!(!sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_local_block() {
        let sync_state = SyncState::default();

        sync_state.set_main_node_block(L2BlockNumber(1));
        sync_state.set_local_block(L2BlockNumber(2));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }

    #[test]
    fn test_sync_state_doesnt_panic_on_main_node_block() {
        let sync_state = SyncState::default();

        sync_state.set_local_block(L2BlockNumber(2));
        sync_state.set_main_node_block(L2BlockNumber(1));
        // ^ should not panic, as we defer the situation to the re-org detector.

        // At the same time, we should consider ourselves synced unless `ReorgDetector` tells us otherwise.
        assert!(sync_state.is_synced());
    }
}
