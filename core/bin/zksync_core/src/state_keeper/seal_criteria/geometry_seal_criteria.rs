use std::fmt::Debug;
use vm::MAX_CYCLES_FOR_TX;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::circuit::GEOMETRY_CONFIG;
use zksync_types::{block::BlockGasCount, tx::ExecutionMetrics};
// Local uses
use crate::state_keeper::seal_criteria::{SealCriterion, SealResolution};

// Collected vm execution metrics should fit into geometry limits.
// Otherwise witness generation will fail and proof won't be generated.

#[derive(Debug, Default)]
pub struct BytecodeHashesCriterion;
#[derive(Debug, Default)]
pub struct RepeatedWritesCriterion;
#[derive(Debug, Default)]
pub struct InitialWritesCriterion;
#[derive(Debug, Default)]
pub struct MaxCyclesCriterion;

trait MetricExtractor {
    const PROM_METRIC_CRITERION_NAME: &'static str;
    fn limit_per_block() -> usize;
    fn extract(metric: &ExecutionMetrics) -> usize;
}

impl<T> SealCriterion for T
where
    T: MetricExtractor + Debug + Send + Sync + 'static,
{
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        block_execution_metrics: ExecutionMetrics,
        tx_execution_metrics: ExecutionMetrics,
        _block_gas_count: BlockGasCount,
        _tx_gas_count: BlockGasCount,
    ) -> SealResolution {
        if T::extract(&tx_execution_metrics)
            > (T::limit_per_block() as f64 * config.reject_tx_at_geometry_percentage).round()
                as usize
        {
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        } else if T::extract(&block_execution_metrics) >= T::limit_per_block() {
            SealResolution::ExcludeAndSeal
        } else if T::extract(&block_execution_metrics)
            > (T::limit_per_block() as f64 * config.close_block_at_geometry_percentage).round()
                as usize
        {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        T::PROM_METRIC_CRITERION_NAME
    }
}

impl MetricExtractor for BytecodeHashesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "used_contract_hashes";

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_code_decommitter_sorter as usize
    }

    fn extract(metrics: &ExecutionMetrics) -> usize {
        metrics.contracts_used
    }
}

impl MetricExtractor for RepeatedWritesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "repeated_storage_writes";

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_repeated_writes_pubdata_hasher as usize
    }

    fn extract(metrics: &ExecutionMetrics) -> usize {
        metrics.repeated_storage_writes
    }
}

impl MetricExtractor for InitialWritesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "initial_storage_writes";

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_initial_writes_pubdata_hasher as usize
    }

    fn extract(metrics: &ExecutionMetrics) -> usize {
        metrics.initial_storage_writes
    }
}

impl MetricExtractor for MaxCyclesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "max_cycles";

    fn limit_per_block() -> usize {
        MAX_CYCLES_FOR_TX as usize
    }

    fn extract(metrics: &ExecutionMetrics) -> usize {
        metrics.cycles_used as usize
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::chain::StateKeeperConfig;
    use zksync_types::tx::ExecutionMetrics;

    use crate::state_keeper::seal_criteria::geometry_seal_criteria::MaxCyclesCriterion;
    use crate::state_keeper::seal_criteria::{SealCriterion, SealResolution};

    use super::{
        BytecodeHashesCriterion, InitialWritesCriterion, MetricExtractor, RepeatedWritesCriterion,
    };

    fn get_config() -> StateKeeperConfig {
        StateKeeperConfig {
            close_block_at_geometry_percentage: 0.9,
            reject_tx_at_geometry_percentage: 0.9,
            ..Default::default()
        }
    }

    fn test_no_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        criterion: &dyn SealCriterion,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            block_execution_metrics,
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(block_resolution, SealResolution::NoSeal);
    }

    fn test_include_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        criterion: &dyn SealCriterion,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            block_execution_metrics,
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(block_resolution, SealResolution::IncludeAndSeal);
    }

    fn test_exclude_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        criterion: &dyn SealCriterion,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            block_execution_metrics,
            Default::default(),
            Default::default(),
            Default::default(),
        );
        assert_eq!(block_resolution, SealResolution::ExcludeAndSeal);
    }

    fn test_unexecutable_tx_resolution(
        tx_execution_metrics: ExecutionMetrics,
        criterion: &dyn SealCriterion,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            Default::default(),
            tx_execution_metrics,
            Default::default(),
            Default::default(),
        );

        assert_eq!(
            block_resolution,
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        );
    }

    macro_rules! test_scenario {
        ($criterion: tt, $metric_name: ident, $metric_type: ty) => {
            let config = get_config();
            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block() / 2) as $metric_type,
                ..Default::default()
            };
            test_no_seal_block_resolution(block_execution_metrics, &$criterion);

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block() - 1) as $metric_type,
                ..Default::default()
            };

            test_include_and_seal_block_resolution(block_execution_metrics, &$criterion);

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block()) as $metric_type,
                ..Default::default()
            };

            test_exclude_and_seal_block_resolution(block_execution_metrics, &$criterion);

            let tx_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block() as f64
                    * config.reject_tx_at_geometry_percentage
                    + 1f64)
                    .round() as $metric_type,
                ..Default::default()
            };

            test_unexecutable_tx_resolution(tx_execution_metrics, &$criterion);
        };
    }

    #[test]
    fn bytecode_hashes_criterion() {
        test_scenario!(BytecodeHashesCriterion, contracts_used, usize);
    }

    #[test]
    fn repeated_writes_seal_criterion() {
        test_scenario!(RepeatedWritesCriterion, repeated_storage_writes, usize);
    }

    #[test]
    fn initial_writes_seal_criterion() {
        test_scenario!(InitialWritesCriterion, initial_storage_writes, usize);
    }

    #[test]
    fn initial_max_cycles_seal_criterion() {
        test_scenario!(MaxCyclesCriterion, cycles_used, u32);
    }
}
