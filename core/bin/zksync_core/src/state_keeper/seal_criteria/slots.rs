use super::{SealCriterion, SealResolution, StateKeeperConfig};
use zksync_types::block::BlockGasCount;
use zksync_types::tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics};

/// Checks whether we should seal the block because we've run out of transaction slots.
#[derive(Debug)]
pub struct SlotsCriterion;

impl SealCriterion for SlotsCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        tx_count: usize,
        _block_execution_metrics: ExecutionMetrics,
        _tx_execution_metrics: ExecutionMetrics,
        _block_gas_count: BlockGasCount,
        _tx_gas_count: BlockGasCount,
        _block_included_txs_size: usize,
        _tx_size: usize,
        _block_writes_metrics: DeduplicatedWritesMetrics,
        _tx_writes_metrics: DeduplicatedWritesMetrics,
    ) -> SealResolution {
        if tx_count >= config.transaction_slots {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "slots"
    }
}

#[cfg(test)]
mod tests {

    use super::{SealCriterion, SealResolution, SlotsCriterion};
    use zksync_config::ZkSyncConfig;

    #[test]
    fn test_slots_seal_criterion() {
        let config = ZkSyncConfig::from_env().chain.state_keeper;
        let criterion = SlotsCriterion;

        let almost_full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            config.transaction_slots - 1,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(almost_full_block_resolution, SealResolution::NoSeal);

        let full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            config.transaction_slots,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);
    }
}
