use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use tokio::time::Instant;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::utils::display_timestamp;

use crate::{
    metrics::AGGREGATION_METRICS,
    utils::{millis_since, millis_since_epoch},
    UpdatesManager,
};

/// I/O-dependent seal criteria.
#[async_trait]
pub trait IoSealCriteria {
    /// Checks whether an L1 batch should be sealed unconditionally (i.e., regardless of metrics
    /// related to transaction execution) given the provided `manager` state.
    async fn should_seal_l1_batch_unconditionally(
        &mut self,
        manager: &UpdatesManager,
    ) -> anyhow::Result<bool>;
    /// Checks whether an L2 block should be sealed given the provided `manager` state.
    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TimeoutSealer {
    block_commit_deadline_ms: u64,
    l2_block_commit_deadline_ms: u64,
}

impl TimeoutSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            block_commit_deadline_ms: config.block_commit_deadline_ms,
            l2_block_commit_deadline_ms: config.l2_block_commit_deadline_ms,
        }
    }
}

#[async_trait]
impl IoSealCriteria for TimeoutSealer {
    async fn should_seal_l1_batch_unconditionally(
        &mut self,
        manager: &UpdatesManager,
    ) -> anyhow::Result<bool> {
        const RULE_NAME: &str = "no_txs_timeout";

        if manager.pending_executed_transactions_len() == 0 {
            // Regardless of which sealers are provided, we never want to seal an empty batch.
            return Ok(false);
        }

        let block_commit_deadline_ms = self.block_commit_deadline_ms;
        // Verify timestamp
        let should_seal_timeout =
            millis_since(manager.batch_timestamp()) > block_commit_deadline_ms;

        if should_seal_timeout {
            AGGREGATION_METRICS.l1_batch_reason_inc_criterion(RULE_NAME);
            tracing::debug!(
                "Decided to seal L1 batch using rule `{RULE_NAME}`; batch timestamp: {}, \
                 commit deadline: {block_commit_deadline_ms}ms",
                display_timestamp(manager.batch_timestamp())
            );
        }
        Ok(should_seal_timeout)
    }

    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        !manager.l2_block.executed_transactions.is_empty()
            && millis_since(manager.l2_block.timestamp) > self.l2_block_commit_deadline_ms
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct L2BlockMaxPayloadSizeSealer {
    max_payload_size: usize,
}

impl L2BlockMaxPayloadSizeSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            max_payload_size: config.l2_block_max_payload_size,
        }
    }

    pub fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        manager.l2_block.payload_encoding_size >= self.max_payload_size
    }
}

/// Seals L1 batch if protocol upgrade is ready to happen.
#[derive(Debug)]
pub(crate) struct ProtocolUpgradeSealer {
    last_checked_at: Option<Instant>,
    check_interval: Duration,
}

impl ProtocolUpgradeSealer {
    pub fn new() -> Self {
        const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(10);

        Self {
            last_checked_at: None,
            check_interval: DEFAULT_CHECK_INTERVAL,
        }
    }

    #[cfg(test)]
    pub fn with_check_interval(mut self, check_interval: Duration) -> Self {
        self.check_interval = check_interval;
        self
    }

    pub async fn should_seal_l1_batch_unconditionally(
        &mut self,
        manager: &UpdatesManager,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        if self
            .last_checked_at
            .is_some_and(|last_check_at| last_check_at.elapsed() < self.check_interval)
        {
            return Ok(false);
        }

        let mut conn = pool.connection_tagged("protocol_upgrade_sealer").await?;
        let current_timestamp = (millis_since_epoch() / 1_000) as u64;
        let protocol_version = conn
            .protocol_versions_dal()
            .protocol_version_id_by_timestamp(current_timestamp)
            .await
            .context("Failed loading protocol version")?;

        self.last_checked_at = Some(Instant::now());

        Ok(protocol_version > manager.protocol_version())
    }
}

#[cfg(test)]
mod tests {
    use zksync_multivm::interface::VmExecutionMetrics;
    use zksync_types::{
        protocol_version::ProtocolSemanticVersion, ProtocolVersion, ProtocolVersionId, Transaction,
    };

    use super::*;
    use crate::tests::{
        create_execution_result, create_transaction, create_updates_manager, seconds_since_epoch,
    };

    fn apply_tx_to_manager(tx: Transaction, manager: &mut UpdatesManager) {
        manager.extend_from_executed_transaction(
            tx,
            create_execution_result([]),
            VmExecutionMetrics::default(),
            vec![],
        );
    }

    /// This test mostly exists to make sure that we can't seal empty L2 blocks on the main node.
    #[test]
    fn timeout_l2_block_sealer() {
        let mut timeout_l2_block_sealer = TimeoutSealer {
            block_commit_deadline_ms: 10_000,
            l2_block_commit_deadline_ms: 10_000,
        };

        let mut manager = create_updates_manager();
        // Empty L2 block should not trigger.
        manager.l2_block.timestamp = seconds_since_epoch() - 10;
        assert!(
            !timeout_l2_block_sealer.should_seal_l2_block(&manager),
            "Empty L2 block shouldn't be sealed"
        );

        // Non-empty L2 block should trigger.
        apply_tx_to_manager(create_transaction(10, 100), &mut manager);
        assert!(
            timeout_l2_block_sealer.should_seal_l2_block(&manager),
            "Non-empty L2 block with old timestamp should be sealed"
        );

        // Check the timestamp logic. This relies on the fact that the test shouldn't run
        // for more than 10 seconds (while the test itself is trivial, it may be preempted
        // by other tests).
        manager.l2_block.timestamp = seconds_since_epoch();
        assert!(
            !timeout_l2_block_sealer.should_seal_l2_block(&manager),
            "Non-empty L2 block with too recent timestamp shouldn't be sealed"
        );
    }

    #[test]
    fn max_size_l2_block_sealer() {
        let tx = create_transaction(10, 100);
        let tx_encoding_size =
            zksync_protobuf::repr::encode::<zksync_dal::consensus::proto::Transaction>(&tx).len();

        let mut max_payload_sealer = L2BlockMaxPayloadSizeSealer {
            max_payload_size: tx_encoding_size,
        };

        let mut manager = create_updates_manager();
        assert!(
            !max_payload_sealer.should_seal_l2_block(&manager),
            "Empty L2 block shouldn't be sealed"
        );

        apply_tx_to_manager(tx, &mut manager);
        assert!(
            max_payload_sealer.should_seal_l2_block(&manager),
            "L2 block with payload encoding size equal or greater than max payload size should be sealed"
        );
    }

    #[tokio::test]
    async fn protocol_upgrade_sealer() {
        const CHECK_INTERVAL: Duration = Duration::from_millis(100);

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut sealer = ProtocolUpgradeSealer::new().with_check_interval(CHECK_INTERVAL);

        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion {
                version: ProtocolSemanticVersion {
                    minor: ProtocolVersionId::latest(),
                    patch: 0.into(),
                },
                ..Default::default()
            })
            .await
            .unwrap();

        let manager = create_updates_manager();

        // No new version, shouldn't seal.
        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager, &pool)
            .await
            .unwrap();
        assert!(!should_seal);

        // Insert new protocol version.
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion {
                version: ProtocolSemanticVersion {
                    minor: ProtocolVersionId::next(),
                    patch: 0.into(),
                },
                ..Default::default()
            })
            .await
            .unwrap();

        let first_checked_at = sealer.last_checked_at.unwrap();
        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager, &pool)
            .await
            .unwrap();
        if first_checked_at == sealer.last_checked_at.unwrap() {
            // This is the expected path, execution should take <CHECK_INTERVAL and sealer should return false.
            assert!(!should_seal);

            // Reset `last_checked_at` and re-check.
            sealer.last_checked_at = Some(first_checked_at - 2 * CHECK_INTERVAL);
            let should_seal = sealer
                .should_seal_l1_batch_unconditionally(&manager, &pool)
                .await
                .unwrap();
            assert!(should_seal);
        } else {
            // Execution took >=CHECK_INTERVAL, it's ok, we don't want to fail the test because of it.
            // Sealer should return true in this case.
            assert!(should_seal);
        }
    }
}
