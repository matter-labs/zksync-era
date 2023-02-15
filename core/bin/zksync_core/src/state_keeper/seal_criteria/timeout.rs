use zksync_types::block::BlockGasCount;
use zksync_types::tx::ExecutionMetrics;
use zksync_utils::time::millis_since_epoch;

use super::{SealCriterion, SealResolution, StateKeeperConfig};

/// Checks whether we should seal the block because we've reached the block commit timeout.
#[derive(Debug)]
pub struct TimeoutCriterion;

impl SealCriterion for TimeoutCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        _block_execution_metrics: ExecutionMetrics,
        _tx_execution_metrics: ExecutionMetrics,
        _block_gas_count: BlockGasCount,
        _tx_gas_count: BlockGasCount,
    ) -> SealResolution {
        if tx_count == 0 {
            return SealResolution::NoSeal;
        }

        let current_timestamp = millis_since_epoch();

        debug_assert!(
            current_timestamp >= block_open_timestamp_ms,
            "We can't go backwards in time"
        );

        if (current_timestamp - block_open_timestamp_ms) as u64 > config.block_commit_deadline_ms {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "seal_criteria_timeout"
    }
}

#[cfg(test)]
mod tests {

    use super::{millis_since_epoch, SealCriterion, SealResolution, TimeoutCriterion};
    use zksync_config::ZkSyncConfig;

    #[test]
    fn test_timeout_seal_criterion() {
        let config = ZkSyncConfig::from_env().chain.state_keeper;
        let criterion = TimeoutCriterion;

        // Empty block shouldn't be sealed by timeout
        let empty_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        // Check criterion workflow
        let no_timeout_resolution = criterion.should_seal(
            &config,
            millis_since_epoch(),
            1,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(no_timeout_resolution, SealResolution::NoSeal);

        let timeout_resolution = criterion.should_seal(
            &config,
            millis_since_epoch() - config.block_commit_deadline_ms as u128 - 1,
            1,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(timeout_resolution, SealResolution::IncludeAndSeal);
    }
}
