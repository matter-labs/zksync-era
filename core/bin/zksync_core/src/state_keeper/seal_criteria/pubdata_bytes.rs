use zksync_types::tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics};
use zksync_types::{block::BlockGasCount, MAX_PUBDATA_PER_L1_BATCH};

use super::{SealCriterion, SealResolution, StateKeeperConfig};

#[derive(Debug)]
pub struct PubDataBytesCriterion;

impl SealCriterion for PubDataBytesCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        block_execution_metrics: ExecutionMetrics,
        tx_execution_metrics: ExecutionMetrics,
        _block_gas_count: BlockGasCount,
        _tx_gas_count: BlockGasCount,
        _block_included_txs_size: usize,
        _tx_size: usize,
        block_writes_metrics: DeduplicatedWritesMetrics,
        tx_writes_metrics: DeduplicatedWritesMetrics,
    ) -> SealResolution {
        let max_pubdata_per_l1_batch = MAX_PUBDATA_PER_L1_BATCH as usize;

        let block_size = block_execution_metrics.size() + block_writes_metrics.size();
        let tx_size = tx_execution_metrics.size() + tx_writes_metrics.size();
        if tx_size
            > (max_pubdata_per_l1_batch as f64 * config.reject_tx_at_eth_params_percentage).round()
                as usize
        {
            SealResolution::Unexecutable(
                "Transaction cannot be sent to L1 due to pubdata limits".into(),
            )
        } else if block_size > max_pubdata_per_l1_batch {
            SealResolution::ExcludeAndSeal
        } else if block_size
            > (max_pubdata_per_l1_batch as f64 * config.close_block_at_eth_params_percentage)
                .round() as usize
        {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "pub_data_size"
    }
}

#[cfg(test)]
mod tests {
    use super::{PubDataBytesCriterion, SealCriterion, SealResolution};
    use crate::state_keeper::seal_criteria::pubdata_bytes::MAX_PUBDATA_PER_L1_BATCH;
    use zksync_config::ZkSyncConfig;
    use zksync_types::tx::ExecutionMetrics;

    #[test]
    fn seal_criterion() {
        let config = ZkSyncConfig::from_env().chain.state_keeper;
        let criterion = PubDataBytesCriterion;

        let block_execution_metrics = ExecutionMetrics {
            contracts_deployed: 0,
            contracts_used: 0,
            gas_used: 0,
            l2_l1_long_messages: (MAX_PUBDATA_PER_L1_BATCH as f64
                * config.close_block_at_eth_params_percentage
                - 1.0)
                .round() as usize,
            published_bytecode_bytes: 0,
            l2_l1_logs: 0,
            vm_events: 0,
            storage_logs: 0,
            total_log_queries: 0,
            cycles_used: 0,
        };

        let empty_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            block_execution_metrics,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        let block_execution_metrics = ExecutionMetrics {
            contracts_deployed: 0,
            contracts_used: 0,
            gas_used: 0,
            l2_l1_long_messages: (MAX_PUBDATA_PER_L1_BATCH as f64
                * config.close_block_at_eth_params_percentage
                + 1f64)
                .round() as usize,
            published_bytecode_bytes: 0,
            l2_l1_logs: 0,
            vm_events: 0,
            storage_logs: 0,
            total_log_queries: 0,
            cycles_used: 0,
        };

        let full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            block_execution_metrics,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);

        let block_execution_metrics = ExecutionMetrics {
            contracts_deployed: 0,
            contracts_used: 0,
            gas_used: 0,
            l2_l1_long_messages: MAX_PUBDATA_PER_L1_BATCH as usize + 1,
            published_bytecode_bytes: 0,
            l2_l1_logs: 0,
            vm_events: 0,
            storage_logs: 0,
            total_log_queries: 0,
            cycles_used: 0,
        };
        let full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            block_execution_metrics,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(full_block_resolution, SealResolution::ExcludeAndSeal);
    }
}
