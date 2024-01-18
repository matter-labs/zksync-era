use std::fmt;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::{tx::tx_execution_info::ExecutionMetrics, ProtocolVersionId};

// Local uses
use crate::state_keeper::seal_criteria::{SealCriterion, SealData, SealResolution};

// Collected vm execution metrics should fit into geometry limits.
// Otherwise witness generation will fail and proof won't be generated.

#[derive(Debug, Default)]
pub struct CircuitsCriterion;

trait MetricExtractor {
    const PROM_METRIC_CRITERION_NAME: &'static str;
    fn limit_per_block(protocol_version: ProtocolVersionId) -> usize;
    fn extract(metric: &ExecutionMetrics) -> usize;
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

        if T::extract(&tx_data.execution_metrics) > reject_bound as usize {
            SealResolution::Unexecutable("ZK proof cannot be generated for a transaction".into())
        } else if T::extract(&block_data.execution_metrics)
            >= T::limit_per_block(protocol_version_id)
        {
            SealResolution::ExcludeAndSeal
        } else if T::extract(&block_data.execution_metrics) > close_bound as usize {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        T::PROM_METRIC_CRITERION_NAME
    }
}

impl MetricExtractor for CircuitsCriterion {
    const PROM_METRIC_CRITERION_NAME: &'static str = "circuits";

    fn limit_per_block(_protocol_version_id: ProtocolVersionId) -> usize {
        // We subtract constant to take into account that circuits may be not fully filled.
        // This constant should be greater than number of circuits types
        // but we keep it larger to be on the safe side.
        const MARGIN_NUMBER_OF_CIRCUITS: usize = 10000;
        const MAX_NUMBER_OF_CIRCUITS: usize = (1 << 14) + (1 << 13) - MARGIN_NUMBER_OF_CIRCUITS;

        MAX_NUMBER_OF_CIRCUITS
    }

    fn extract(metrics: &ExecutionMetrics) -> usize {
        metrics.estimated_circuits_used.ceil() as usize
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
                ..SealData::default()
            },
            &SealData::default(),
            protocol_version,
        );
        assert_eq!(block_resolution, SealResolution::NoSeal);
    }

    fn test_include_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
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
                ..SealData::default()
            },
            &SealData::default(),
            protocol_version,
        );
        assert_eq!(block_resolution, SealResolution::IncludeAndSeal);
    }

    fn test_exclude_and_seal_block_resolution(
        block_execution_metrics: ExecutionMetrics,
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
                ..SealData::default()
            },
            &SealData::default(),
            protocol_version,
        );
        assert_eq!(block_resolution, SealResolution::ExcludeAndSeal);
    }

    fn test_unexecutable_tx_resolution(
        tx_execution_metrics: ExecutionMetrics,
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
            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) / 2) as $metric_type,
                ..ExecutionMetrics::default()
            };
            test_no_seal_block_resolution(block_execution_metrics, &$criterion, $protocol_version);

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version) - 1) as $metric_type,
                ..ExecutionMetrics::default()
            };

            test_include_and_seal_block_resolution(
                block_execution_metrics,
                &$criterion,
                $protocol_version,
            );

            let block_execution_metrics = ExecutionMetrics {
                $metric_name: ($criterion::limit_per_block($protocol_version)) as $metric_type,
                ..ExecutionMetrics::default()
            };

            test_exclude_and_seal_block_resolution(
                block_execution_metrics,
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

            test_unexecutable_tx_resolution(tx_execution_metrics, &$criterion, $protocol_version);
        };
    }

    #[test]
    fn computational_gas_seal_criterion() {
        test_scenario_execution_metrics!(
            CircuitsCriterion,
            estimated_circuits_used,
            f32,
            ProtocolVersionId::Version17
        );
    }
}
