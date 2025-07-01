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
    l1_batch_commit_deadline_ms: u64,
    l2_block_commit_deadline_ms: u64,
}

impl TimeoutSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            l1_batch_commit_deadline_ms: config.l1_batch_commit_deadline.as_millis() as u64,
            l2_block_commit_deadline_ms: config.l2_block_commit_deadline.as_millis() as u64,
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

        let l1_batch_commit_deadline_ms = self.l1_batch_commit_deadline_ms;
        // Verify timestamp
        let should_seal_timeout =
            millis_since(manager.batch_timestamp()) > l1_batch_commit_deadline_ms;

        if should_seal_timeout {
            AGGREGATION_METRICS.l1_batch_reason_inc_criterion(RULE_NAME);
            tracing::debug!(
                "Decided to seal L1 batch using rule `{RULE_NAME}`; batch timestamp: {}, \
                 commit deadline: {l1_batch_commit_deadline_ms}ms",
                display_timestamp(manager.batch_timestamp())
            );
        }
        Ok(should_seal_timeout)
    }

    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        !manager.l2_block.executed_transactions.is_empty()
            && millis_since_epoch().saturating_sub(manager.l2_block.timestamp_ms())
                > self.l2_block_commit_deadline_ms
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct L2BlockMaxPayloadSizeSealer {
    max_payload_size: usize,
}

impl L2BlockMaxPayloadSizeSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            max_payload_size: config.l2_block_max_payload_size.0 as usize,
        }
    }

    pub fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        manager.l2_block.payload_encoding_size >= self.max_payload_size
    }
}

/// Seals L1 batch if pending protocol upgrade is ready to happen or
/// if the batch is the first one after protocol upgrade.
/// First condition is checked once per `check_interval` to reduce number of DB queries.
#[derive(Debug)]
pub(crate) struct ProtocolUpgradeSealer {
    last_checked_at: Option<Instant>,
    check_interval: Duration,
    pool: ConnectionPool<Core>,
}

impl ProtocolUpgradeSealer {
    pub fn new(pool: ConnectionPool<Core>) -> Self {
        const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(10);

        Self {
            last_checked_at: None,
            check_interval: DEFAULT_CHECK_INTERVAL,
            pool,
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
    ) -> anyhow::Result<bool> {
        if manager.pending_executed_transactions_len() == 0 {
            return Ok(false);
        }

        if manager.protocol_version() != manager.previous_batch_protocol_version() {
            AGGREGATION_METRICS.l1_batch_reason_inc_criterion("first_batch_after_upgrade");
            return Ok(true);
        }

        if self
            .last_checked_at
            .is_some_and(|last_check_at| last_check_at.elapsed() < self.check_interval)
        {
            return Ok(false);
        }

        let mut conn = self
            .pool
            .connection_tagged("protocol_upgrade_sealer")
            .await?;
        let current_timestamp = millis_since_epoch() / 1_000;
        let protocol_version = conn
            .protocol_versions_dal()
            .protocol_version_id_by_timestamp(current_timestamp)
            .await
            .context("Failed loading protocol version")?;
        let should_seal = protocol_version != manager.protocol_version();
        if should_seal {
            AGGREGATION_METRICS.l1_batch_reason_inc_criterion("last_batch_before_upgrade");
        }
        self.last_checked_at = Some(Instant::now());

        Ok(should_seal)
    }
}

#[cfg(test)]
mod tests {
    use zksync_contracts::BaseSystemContracts;
    use zksync_multivm::{
        interface::{SystemEnv, TxExecutionMode, VmExecutionMetrics},
        zk_evm_latest::ethereum_types::Address,
    };
    use zksync_node_test_utils::default_l1_batch_env;
    use zksync_system_constants::ZKPORTER_IS_AVAILABLE;
    use zksync_types::{
        protocol_version::ProtocolSemanticVersion, L2ChainId, ProtocolVersion, ProtocolVersionId,
        Transaction,
    };

    use super::*;
    use crate::{
        io::BatchInitParams,
        tests::{create_execution_result, create_transaction, create_updates_manager},
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
            l1_batch_commit_deadline_ms: 10_000,
            l2_block_commit_deadline_ms: 10_000,
        };

        let mut manager = create_updates_manager();
        // Empty L2 block should not trigger.
        manager
            .l2_block
            .set_timestamp_ms(millis_since_epoch() - 10_001);
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
        manager.l2_block.set_timestamp_ms(millis_since_epoch());
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
        let mut sealer =
            ProtocolUpgradeSealer::new(pool.clone()).with_check_interval(CHECK_INTERVAL);

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

        let mut manager = create_updates_manager();
        let tx = create_transaction(10, 100);
        apply_tx_to_manager(tx, &mut manager);

        // No new version, shouldn't seal.
        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager)
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

        // Reset `last_checked_at` so that the check is skipped and sealer returns false.
        sealer.last_checked_at = Some(Instant::now());
        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager)
            .await
            .unwrap();
        assert!(!should_seal);

        // Reset `last_checked_at` so that sealer does the check.
        sealer.last_checked_at = Some(Instant::now() - 2 * CHECK_INTERVAL);
        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager)
            .await
            .unwrap();
        assert!(should_seal);

        // And now check how the first batch with the new protocol version will be treated.
        let l1_batch_env = default_l1_batch_env(1, 1, Address::default());
        let system_env = SystemEnv {
            zk_porter_available: ZKPORTER_IS_AVAILABLE,
            version: ProtocolVersionId::next(),
            base_system_smart_contracts: BaseSystemContracts::load_from_disk(),
            bootloader_gas_limit: u32::MAX,
            execution_mode: TxExecutionMode::VerifyExecute,
            default_validation_computational_gas_limit: u32::MAX,
            chain_id: L2ChainId::from(270),
        };
        let timestamp_ms = l1_batch_env.first_l2_block.timestamp * 1000;
        let mut manager = UpdatesManager::new(
            &BatchInitParams {
                l1_batch_env,
                system_env,
                pubdata_params: Default::default(),
                pubdata_limit: Some(100_000),
                timestamp_ms,
            },
            ProtocolVersionId::latest(),
        );
        // No txs, should not be sealed.
        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager)
            .await
            .unwrap();
        assert!(!should_seal);

        // Add tx and check that should be sealed.
        let tx = create_transaction(10, 100);
        apply_tx_to_manager(tx, &mut manager);

        let should_seal = sealer
            .should_seal_l1_batch_unconditionally(&manager)
            .await
            .unwrap();
        assert!(should_seal);
    }
}
