use std::fmt::Debug;
use vm::{zk_evm::zkevm_opcode_defs::system_params::ERGS_PER_CIRCUIT, MAX_CYCLES_FOR_TX};
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::circuit::GEOMETRY_CONFIG;
use zksync_types::{
    block::BlockGasCount,
    circuit::SCHEDULER_UPPER_BOUND,
    tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics},
};
// Local uses
use crate::state_keeper::seal_criteria::{SealCriterion, SealResolution};

// Collected vm execution metrics should fit into geometry limits.
// Otherwise witness generation will fail and proof won't be generated.

#[derive(Debug, Default)]
pub struct RepeatedWritesCriterion;
#[derive(Debug, Default)]
pub struct InitialWritesCriterion;
#[derive(Debug, Default)]
pub struct MaxCyclesCriterion;
#[derive(Debug, Default)]
pub struct ComputationalGasCriterion;

trait MetricExtractor {
    const PROM_METRIC_CRITERION_NAME: &'static str;
    fn limit_per_block() -> usize;
    fn extract(metric: &ExecutionMetrics, writes: &DeduplicatedWritesMetrics) -> usize;
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
        _block_included_txs_size: usize,
        _tx_size: usize,
        block_writes_metrics: DeduplicatedWritesMetrics,
        tx_writes_metrics: DeduplicatedWritesMetrics,
    ) -> SealResolution {
        if T::extract(&tx_execution_metrics, &tx_writes_metrics)
            > (T::limit_per_block() as f64 * config.reject_tx_at_geometry_percentage).round()
                as usize
        {
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        } else if T::extract(&block_execution_metrics, &block_writes_metrics)
            >= T::limit_per_block()
        {
            SealResolution::ExcludeAndSeal
        } else if T::extract(&block_execution_metrics, &block_writes_metrics)
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

impl MetricExtractor for RepeatedWritesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "repeated_storage_writes";

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_repeated_writes_pubdata_hasher as usize
    }

    fn extract(_metrics: &ExecutionMetrics, writes: &DeduplicatedWritesMetrics) -> usize {
        writes.repeated_storage_writes
    }
}

impl MetricExtractor for InitialWritesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "initial_storage_writes";

    fn limit_per_block() -> usize {
        GEOMETRY_CONFIG.limit_for_initial_writes_pubdata_hasher as usize
    }

    fn extract(_metrics: &ExecutionMetrics, writes: &DeduplicatedWritesMetrics) -> usize {
        writes.initial_storage_writes
    }
}

impl MetricExtractor for MaxCyclesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "max_cycles";

    fn limit_per_block() -> usize {
        MAX_CYCLES_FOR_TX as usize
    }

    fn extract(metrics: &ExecutionMetrics, _writes: &DeduplicatedWritesMetrics) -> usize {
        metrics.cycles_used as usize
    }
}

impl MetricExtractor for ComputationalGasCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "computational_gas";

    fn limit_per_block() -> usize {
        // We subtract constant to take into account that circuits may be not fully filled.
        // This constant should be greater than number of circuits types
        // but we keep it larger to be on the safe side.
        const MARGIN_NUMBER_OF_CIRCUITS: usize = 100;
        const MAX_NUMBER_OF_MUTLIINSTANCE_CIRCUITS: usize =
            SCHEDULER_UPPER_BOUND as usize - MARGIN_NUMBER_OF_CIRCUITS;

        MAX_NUMBER_OF_MUTLIINSTANCE_CIRCUITS * ERGS_PER_CIRCUIT as usize
    }

    fn extract(metrics: &ExecutionMetrics, _writes: &DeduplicatedWritesMetrics) -> usize {
        metrics.computational_gas_used as usize
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::chain::StateKeeperConfig;
    use zksync_types::tx::tx_execution_info::DeduplicatedWritesMetrics;
    use zksync_types::tx::ExecutionMetrics;

    use super::{
        ComputationalGasCriterion, InitialWritesCriterion, MaxCyclesCriterion, MetricExtractor,
        RepeatedWritesCriterion,
    };
    use crate::state_keeper::seal_criteria::{SealCriterion, SealResolution};

    fn get_config() -> StateKeeperConfig {
        StateKeeperConfig {
            close_block_at_geometry_percentage: 0.9,
            reject_tx_at_geometry_percentage: 0.9,
            ..Default::default()
        }
    }

    fn test_no_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        block_writes_metrics: DeduplicatedWritesMetrics,
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
            Default::default(),
            Default::default(),
            block_writes_metrics,
            Default::default(),
        );
        assert_eq!(block_resolution, SealResolution::NoSeal);
    }

    fn test_include_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        block_writes_metrics: DeduplicatedWritesMetrics,
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
            Default::default(),
            Default::default(),
            block_writes_metrics,
            Default::default(),
        );
        assert_eq!(block_resolution, SealResolution::IncludeAndSeal);
    }

    fn test_exclude_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        block_writes_metrics: DeduplicatedWritesMetrics,
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
            Default::default(),
            Default::default(),
            block_writes_metrics,
            Default::default(),
        );
        assert_eq!(block_resolution, SealResolution::ExcludeAndSeal);
    }

    fn test_unexecutable_tx_resolution(
        tx_execution_metrics: ExecutionMetrics,
        tx_writes_metrics: DeduplicatedWritesMetrics,
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
            Default::default(),
            Default::default(),
            Default::default(),
            tx_writes_metrics,
        );

        assert_eq!(
            block_resolution,
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        );
    }

    macro_rules! test_scenario_execution_metrics {
        ($criterion: tt, $metric_name: ident, $metric_type: ty) => {
            let config = get_config();
            let writes_metrics = DeduplicatedWritesMetrics::default();
            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block() / 2) as $metric_type,
                ..Default::default()
            };
            test_no_seal_block_resolution(block_execution_metrics, writes_metrics, &$criterion);

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block() - 1) as $metric_type,
                ..Default::default()
            };

            test_include_and_seal_block_resolution(
                block_execution_metrics,
                writes_metrics,
                &$criterion,
            );

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block()) as $metric_type,
                ..Default::default()
            };

            test_exclude_and_seal_block_resolution(
                block_execution_metrics,
                writes_metrics,
                &$criterion,
            );

            let tx_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block() as f64
                    * config.reject_tx_at_geometry_percentage
                    + 1f64)
                    .round() as $metric_type,
                ..Default::default()
            };

            test_unexecutable_tx_resolution(tx_execution_metrics, writes_metrics, &$criterion);
        };
    }

    macro_rules! test_scenario_writes_metrics {
        ($criterion: tt, $metric_name: ident, $metric_type: ty) => {
            let config = get_config();
            let execution_metrics = ExecutionMetrics::default();
            let block_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block() / 2) as $metric_type,
                ..Default::default()
            };
            test_no_seal_block_resolution(execution_metrics, block_writes_metrics, &$criterion);

            let block_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block() - 1) as $metric_type,
                ..Default::default()
            };

            test_include_and_seal_block_resolution(
                execution_metrics,
                block_writes_metrics,
                &$criterion,
            );

            let block_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block()) as $metric_type,
                ..Default::default()
            };

            test_exclude_and_seal_block_resolution(
                execution_metrics,
                block_writes_metrics,
                &$criterion,
            );

            let tx_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block() as f64
                    * config.reject_tx_at_geometry_percentage
                    + 1f64)
                    .round() as $metric_type,
                ..Default::default()
            };

            test_unexecutable_tx_resolution(execution_metrics, tx_writes_metrics, &$criterion);
        };
    }

    #[test]
    fn repeated_writes_seal_criterion() {
        test_scenario_writes_metrics!(RepeatedWritesCriterion, repeated_storage_writes, usize);
    }

    #[test]
    fn initial_writes_seal_criterion() {
        test_scenario_writes_metrics!(InitialWritesCriterion, initial_storage_writes, usize);
    }

    #[test]
    fn max_cycles_seal_criterion() {
        test_scenario_execution_metrics!(MaxCyclesCriterion, cycles_used, u32);
    }

    #[test]
    fn computational_gas_seal_criterion() {
        test_scenario_execution_metrics!(ComputationalGasCriterion, computational_gas_used, u32);
    }
}
