use multivm::vm_latest::constants::{ERGS_PER_CIRCUIT, MAX_CYCLES_FOR_TX};
use std::fmt;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::{
    circuit::{GEOMETRY_CONFIG, SCHEDULER_UPPER_BOUND},
    tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics},
    ProtocolVersionId,
};

// Local uses
use crate::state_keeper::seal_criteria::{SealCriterion, SealData, SealResolution};

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
#[derive(Debug, Default)]
pub struct L2ToL1LogsCriterion;

trait MetricExtractor {
    const PROM_METRIC_CRITERION_NAME: &'static str;
    fn limit_per_block(protocol_version: ProtocolVersionId) -> usize;
    fn extract(metric: &ExecutionMetrics, writes: &DeduplicatedWritesMetrics) -> usize;
}

impl<T> SealCriterion for T
where
    T: MetricExtractor + fmt::Debug + Send + Sync + 'static,
{
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version_id: ProtocolVersionId,
    ) -> SealResolution {
        let reject_bound = (T::limit_per_block(protocol_version_id) as f64
            * config.reject_tx_at_geometry_percentage)
            .round();
        let close_bound = (T::limit_per_block(protocol_version_id) as f64
            * config.close_block_at_geometry_percentage)
            .round();

        if T::extract(&tx_data.execution_metrics, &tx_data.writes_metrics) > reject_bound as usize {
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        } else if T::extract(&block_data.execution_metrics, &block_data.writes_metrics)
            >= T::limit_per_block(protocol_version_id)
        {
            SealResolution::ExcludeAndSeal
        } else if T::extract(&block_data.execution_metrics, &block_data.writes_metrics)
            > close_bound as usize
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

    fn limit_per_block(protocol_version_id: ProtocolVersionId) -> usize {
        if protocol_version_id.is_pre_boojum() {
            GEOMETRY_CONFIG.limit_for_repeated_writes_pubdata_hasher as usize
        } else {
            // In boojum there is no limit for repeated writes.
            usize::MAX
        }
    }

    fn extract(_metrics: &ExecutionMetrics, writes: &DeduplicatedWritesMetrics) -> usize {
        writes.repeated_storage_writes
    }
}

impl MetricExtractor for InitialWritesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "initial_storage_writes";

    fn limit_per_block(protocol_version_id: ProtocolVersionId) -> usize {
        if protocol_version_id.is_pre_boojum() {
            GEOMETRY_CONFIG.limit_for_initial_writes_pubdata_hasher as usize
        } else {
            // In boojum there is no limit for initial writes.
            usize::MAX
        }
    }

    fn extract(_metrics: &ExecutionMetrics, writes: &DeduplicatedWritesMetrics) -> usize {
        writes.initial_storage_writes
    }
}

impl MetricExtractor for MaxCyclesCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "max_cycles";

    fn limit_per_block(_protocol_version_id: ProtocolVersionId) -> usize {
        MAX_CYCLES_FOR_TX as usize
    }

    fn extract(metrics: &ExecutionMetrics, _writes: &DeduplicatedWritesMetrics) -> usize {
        metrics.cycles_used as usize
    }
}

impl MetricExtractor for ComputationalGasCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "computational_gas";

    fn limit_per_block(_protocol_version_id: ProtocolVersionId) -> usize {
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

impl MetricExtractor for L2ToL1LogsCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "l2_to_l1_logs";

    fn limit_per_block(protocol_version_id: ProtocolVersionId) -> usize {
        if protocol_version_id.is_pre_boojum() {
            GEOMETRY_CONFIG.limit_for_l1_messages_merklizer as usize
        } else {
            // In boojum there is no limit for L2 to L1 logs.
            usize::MAX
        }
    }

    fn extract(metrics: &ExecutionMetrics, _writes: &DeduplicatedWritesMetrics) -> usize {
        metrics.l2_to_l1_logs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            &SealData {
                execution_metrics: block_execution_metrics,
                writes_metrics: block_writes_metrics,
                ..SealData::default()
            },
            &SealData::default(),
            protocol_version,
        );
        assert_eq!(block_resolution, SealResolution::NoSeal);
    }

    fn test_include_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        block_writes_metrics: DeduplicatedWritesMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            &SealData {
                execution_metrics: block_execution_metrics,
                writes_metrics: block_writes_metrics,
                ..SealData::default()
            },
            &SealData::default(),
            protocol_version,
        );
        assert_eq!(block_resolution, SealResolution::IncludeAndSeal);
    }

    fn test_exclude_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
        block_writes_metrics: DeduplicatedWritesMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            &SealData {
                execution_metrics: block_execution_metrics,
                writes_metrics: block_writes_metrics,
                ..SealData::default()
            },
            &SealData::default(),
            protocol_version,
        );
        assert_eq!(block_resolution, SealResolution::ExcludeAndSeal);
    }

    fn test_unexecutable_tx_resolution(
        tx_execution_metrics: ExecutionMetrics,
        tx_writes_metrics: DeduplicatedWritesMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            0,
            &SealData::default(),
            &SealData {
                execution_metrics: tx_execution_metrics,
                writes_metrics: tx_writes_metrics,
                ..SealData::default()
            },
            protocol_version,
        );

        assert_eq!(
            block_resolution,
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        );
    }

    macro_rules! test_scenario_execution_metrics {
        ($criterion: tt, $metric_name: ident, $metric_type: ty, $protocol_version: expr) => {
            let config = get_config();
            let writes_metrics = DeduplicatedWritesMetrics::default();
            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) / 2) as $metric_type,
                ..ExecutionMetrics::default()
            };
            test_no_seal_block_resolution(
                block_execution_metrics,
                writes_metrics,
                &$criterion,
                $protocol_version,
            );

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) - 1) as $metric_type,
                ..ExecutionMetrics::default()
            };

            test_include_and_seal_block_resolution(
                block_execution_metrics,
                writes_metrics,
                &$criterion,
                $protocol_version,
            );

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version)) as $metric_type,
                ..ExecutionMetrics::default()
            };

            test_exclude_and_seal_block_resolution(
                block_execution_metrics,
                writes_metrics,
                &$criterion,
                $protocol_version,
            );

            let tx_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) as f64
                    * config.reject_tx_at_geometry_percentage
                    + 1f64)
                    .round() as $metric_type,
                ..ExecutionMetrics::default()
            };

            test_unexecutable_tx_resolution(
                tx_execution_metrics,
                writes_metrics,
                &$criterion,
                $protocol_version,
            );
        };
    }

    macro_rules! test_scenario_writes_metrics {
        ($criterion:tt, $metric_name:ident, $metric_type:ty, $protocol_version:expr) => {
            let config = get_config();
            let execution_metrics = ExecutionMetrics::default();
            let block_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) / 2) as $metric_type,
                ..Default::default()
            };
            test_no_seal_block_resolution(
                execution_metrics,
                block_writes_metrics,
                &$criterion,
                $protocol_version,
            );

            let block_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) - 1) as $metric_type,
                ..Default::default()
            };

            test_include_and_seal_block_resolution(
                execution_metrics,
                block_writes_metrics,
                &$criterion,
                $protocol_version,
            );

            let block_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version)) as $metric_type,
                ..Default::default()
            };

            test_exclude_and_seal_block_resolution(
                execution_metrics,
                block_writes_metrics,
                &$criterion,
                $protocol_version,
            );

            let tx_writes_metrics = DeduplicatedWritesMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) as f64
                    * config.reject_tx_at_geometry_percentage
                    + 1f64)
                    .round() as $metric_type,
                ..Default::default()
            };

            test_unexecutable_tx_resolution(
                execution_metrics,
                tx_writes_metrics,
                &$criterion,
                $protocol_version,
            );
        };
    }

    #[test]
    fn repeated_writes_seal_criterion() {
        test_scenario_writes_metrics!(
            RepeatedWritesCriterion,
            repeated_storage_writes,
            usize,
            ProtocolVersionId::Version17
        );
    }

    #[test]
    fn initial_writes_seal_criterion() {
        test_scenario_writes_metrics!(
            InitialWritesCriterion,
            initial_storage_writes,
            usize,
            ProtocolVersionId::Version17
        );
    }

    #[test]
    fn max_cycles_seal_criterion() {
        test_scenario_execution_metrics!(
            MaxCyclesCriterion,
            cycles_used,
            u32,
            ProtocolVersionId::Version17
        );
    }

    #[test]
    fn computational_gas_seal_criterion() {
        test_scenario_execution_metrics!(
            ComputationalGasCriterion,
            computational_gas_used,
            u32,
            ProtocolVersionId::Version17
        );
    }

    #[test]
    fn l2_to_l1_logs_seal_criterion() {
        test_scenario_execution_metrics!(
            L2ToL1LogsCriterion,
            l2_to_l1_logs,
            usize,
            ProtocolVersionId::Version17
        );
    }
}
