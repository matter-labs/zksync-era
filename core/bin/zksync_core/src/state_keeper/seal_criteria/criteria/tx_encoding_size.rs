use vm::vm_with_bootloader::BOOTLOADER_TX_ENCODING_SPACE;
use zksync_types::block::BlockGasCount;
use zksync_types::tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics};

use crate::state_keeper::seal_criteria::{SealCriterion, SealResolution, StateKeeperConfig};

#[derive(Debug)]
pub struct TxEncodingSizeCriterion;

impl SealCriterion for TxEncodingSizeCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        _block_execution_metrics: ExecutionMetrics,
        _tx_execution_metrics: ExecutionMetrics,
        _block_gas_count: BlockGasCount,
        _tx_gas_count: BlockGasCount,
        block_included_txs_size: usize,
        tx_size: usize,
        _block_writes_metrics: DeduplicatedWritesMetrics,
        _tx_writes_metrics: DeduplicatedWritesMetrics,
    ) -> SealResolution {
        if tx_size
            > (BOOTLOADER_TX_ENCODING_SPACE as f64 * config.reject_tx_at_geometry_percentage)
                .round() as usize
        {
            SealResolution::Unexecutable(
                "Transaction cannot be included due to large encoding size".into(),
            )
        } else if block_included_txs_size > BOOTLOADER_TX_ENCODING_SPACE as usize {
            SealResolution::ExcludeAndSeal
        } else if block_included_txs_size
            > (BOOTLOADER_TX_ENCODING_SPACE as f64 * config.close_block_at_geometry_percentage)
                .round() as usize
        {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "tx_encoding_size"
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SealCriterion, SealResolution, TxEncodingSizeCriterion, BOOTLOADER_TX_ENCODING_SPACE,
    };
    use zksync_config::ZkSyncConfig;

    #[test]
    fn seal_criterion() {
        let config = ZkSyncConfig::from_env().chain.state_keeper;
        let criterion = TxEncodingSizeCriterion;

        let empty_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        let unexecutable_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            BOOTLOADER_TX_ENCODING_SPACE as usize + 1,
            Default::default(),
            Default::default(),
        );
        assert_eq!(
            unexecutable_resolution,
            SealResolution::Unexecutable(
                "Transaction cannot be included due to large encoding size".into()
            )
        );

        let exclude_and_seal_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            BOOTLOADER_TX_ENCODING_SPACE as usize + 1,
            1,
            Default::default(),
            Default::default(),
        );
        assert_eq!(exclude_and_seal_resolution, SealResolution::ExcludeAndSeal);

        let include_and_seal_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            BOOTLOADER_TX_ENCODING_SPACE as usize,
            1,
            Default::default(),
            Default::default(),
        );
        assert_eq!(include_and_seal_resolution, SealResolution::IncludeAndSeal);
    }
}
