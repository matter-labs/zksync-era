use super::{SealCriterion, SealResolution, StateKeeperConfig};
use crate::gas_tracker::new_block_gas_count;
use zksync_types::block::BlockGasCount;
use zksync_types::tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics};

/// This is a temporary solution
/// Instead of checking for gas it simply checks that the contracts'
/// bytecode is large enough.
/// Among all the data which will be published on-chain the contracts'
/// bytecode is by far the largest one and with high probability
/// the slots will run out before the other pubdata becomes too big
#[derive(Debug)]
pub(crate) struct GasCriterion;

impl SealCriterion for GasCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        _block_execution_metrics: ExecutionMetrics,
        _tx_execution_metrics: ExecutionMetrics,
        block_gas_count: BlockGasCount,
        tx_gas_count: BlockGasCount,
        _block_included_txs_size: usize,
        _tx_size: usize,
        _block_writes_metrics: DeduplicatedWritesMetrics,
        _tx_writes_metrics: DeduplicatedWritesMetrics,
    ) -> SealResolution {
        if (tx_gas_count + new_block_gas_count()).has_greater_than(
            (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round() as u32,
        ) {
            SealResolution::Unexecutable("Transaction requires too much gas".into())
        } else if block_gas_count.has_greater_than(config.max_single_tx_gas) {
            SealResolution::ExcludeAndSeal
        } else if block_gas_count.has_greater_than(
            (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage).round() as u32,
        ) {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "gas"
    }
}

#[cfg(test)]
mod tests {

    use super::{new_block_gas_count, BlockGasCount, GasCriterion, SealCriterion, SealResolution};
    use zksync_config::ZkSyncConfig;

    #[test]
    fn test_gas_seal_criterion() {
        let config = ZkSyncConfig::from_env().chain.state_keeper;
        let criterion = GasCriterion;

        // Empty block should fit into gas criterion.
        let empty_block_gas = new_block_gas_count();
        let empty_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            empty_block_gas,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);
        let tx_gas = BlockGasCount {
            commit: config.max_single_tx_gas + 1,
            prove: 0,
            execute: 0,
        };
        // Transaction that needs more gas than a block limit should be unexecutable.
        let huge_transaction_resolution = criterion.should_seal(
            &config,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            empty_block_gas + tx_gas,
            tx_gas,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(
            huge_transaction_resolution,
            SealResolution::Unexecutable("Transaction requires too much gas".into())
        );

        // Check criterion workflow
        let tx_gas = BlockGasCount {
            commit: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.commit,
            prove: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.prove,
            execute: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.execute,
        };
        let resolution_after_first_tx = criterion.should_seal(
            &config,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            empty_block_gas + tx_gas,
            tx_gas,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(resolution_after_first_tx, SealResolution::NoSeal);

        // Check criterion workflow
        let tx_gas = BlockGasCount {
            commit: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.commit
                - 1,
            prove: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.prove
                - 1,
            execute: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.execute
                - 1,
        };

        let block_gas = BlockGasCount {
            commit: (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage).round()
                as u32
                + 1,
            prove: (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage).round()
                as u32
                + 1,
            execute: (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage)
                .round() as u32
                + 1,
        };
        let resolution_after_first_tx = criterion.should_seal(
            &config,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            block_gas,
            tx_gas,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(resolution_after_first_tx, SealResolution::IncludeAndSeal);

        // Check criterion workflow
        let tx_gas = BlockGasCount {
            commit: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.commit,
            prove: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.prove,
            execute: (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round()
                as u32
                - empty_block_gas.execute,
        };
        let resolution_after_first_tx = criterion.should_seal(
            &config,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            empty_block_gas + tx_gas + tx_gas,
            tx_gas,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(resolution_after_first_tx, SealResolution::ExcludeAndSeal);
    }
}
