use zksync_config::configs::chain::SealCriteriaConfig;
use zksync_multivm::utils::{
    circuit_statistics_bootloader_batch_tip_overhead, get_max_batch_base_layer_circuits,
};
use zksync_types::ProtocolVersionId;

// Local uses
use crate::seal_criteria::{SealCriterion, SealData, SealResolution, UnexecutableReason};

// Collected vm execution metrics should fit into geometry limits.
// Otherwise witness generation will fail and proof won't be generated.

/// Checks whether we should exclude the transaction because we don't have enough circuits for it.
#[derive(Debug)]
pub struct CircuitsCriterion;

impl SealCriterion for CircuitsCriterion {
    fn should_seal(
        &self,
        config: &SealCriteriaConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let max_allowed_base_layer_circuits =
            get_max_batch_base_layer_circuits(protocol_version.into());
        if config.max_circuits_per_batch > max_allowed_base_layer_circuits {
            tracing::warn!(
                "Configured max_circuits_per_batch ({}) is greater than the constant MAX_BASE_LAYER_CIRCUITS={} for protocol version {}. \
                 Configured value has no effect, MAX_BASE_LAYER_CIRCUITS is used as a limit for seal criteria. Consider updating configured value.",
                config.max_circuits_per_batch, max_allowed_base_layer_circuits, protocol_version as u16
            );
        }

        let effective_max_circuits_per_batch =
            max_allowed_base_layer_circuits.min(config.max_circuits_per_batch);

        let batch_tip_circuit_overhead =
            circuit_statistics_bootloader_batch_tip_overhead(protocol_version.into());

        // Double checking that it is possible to seal batches
        assert!(
            batch_tip_circuit_overhead < effective_max_circuits_per_batch,
            "Invalid circuit criteria"
        );

        let reject_bound = (effective_max_circuits_per_batch as f64
            * config.reject_tx_at_geometry_percentage)
            .round() as usize;
        let include_and_seal_bound = (effective_max_circuits_per_batch as f64
            * config.close_block_at_geometry_percentage)
            .round() as usize;

        let used_circuits_tx = tx_data.execution_metrics.circuit_statistic.total();
        let used_circuits_batch = block_data.execution_metrics.circuit_statistic.total();

        if used_circuits_tx + batch_tip_circuit_overhead >= reject_bound {
            UnexecutableReason::ProofWillFail.into()
        } else if used_circuits_batch + batch_tip_circuit_overhead
            >= effective_max_circuits_per_batch
        {
            SealResolution::ExcludeAndSeal
        } else if used_circuits_batch + batch_tip_circuit_overhead >= include_and_seal_bound {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn capacity_filled(
        &self,
        config: &SealCriteriaConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        let batch_tip_circuit_overhead =
            circuit_statistics_bootloader_batch_tip_overhead(protocol_version.into());
        let used = (block_data.execution_metrics.circuit_statistic.total()
            + batch_tip_circuit_overhead) as f64;
        let full = config.max_circuits_per_batch as f64;
        Some(used / full)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "circuits_criterion"
    }
}
#[cfg(test)]
mod tests {
    use zksync_multivm::interface::{CircuitStatistic, VmExecutionMetrics};

    use super::*;

    const MAX_CIRCUITS_PER_BATCH: usize = 27_000;

    fn get_config() -> SealCriteriaConfig {
        SealCriteriaConfig {
            close_block_at_geometry_percentage: 0.9,
            reject_tx_at_geometry_percentage: 0.9,
            max_circuits_per_batch: MAX_CIRCUITS_PER_BATCH,
            ..SealCriteriaConfig::for_tests()
        }
    }

    fn test_no_seal_block_resolution(
        block_execution_metrics: VmExecutionMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            0,
            0,
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
        block_execution_metrics: VmExecutionMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            0,
            0,
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
        block_execution_metrics: VmExecutionMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            0,
            0,
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
        tx_execution_metrics: VmExecutionMetrics,
        criterion: &dyn SealCriterion,
        protocol_version: ProtocolVersionId,
    ) {
        let config = get_config();
        let block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            0,
            &SealData::default(),
            &SealData {
                execution_metrics: tx_execution_metrics,
                ..SealData::default()
            },
            protocol_version,
        );

        assert_eq!(block_resolution, UnexecutableReason::ProofWillFail.into());
    }

    #[test]
    fn circuits_seal_criterion() {
        let config = get_config();
        let protocol_version = ProtocolVersionId::latest();
        let block_execution_metrics = VmExecutionMetrics {
            circuit_statistic: CircuitStatistic {
                main_vm: (MAX_CIRCUITS_PER_BATCH / 4) as f32,
                ..CircuitStatistic::default()
            },
            ..VmExecutionMetrics::default()
        };
        test_no_seal_block_resolution(
            block_execution_metrics,
            &CircuitsCriterion,
            protocol_version,
        );

        let block_execution_metrics = VmExecutionMetrics {
            circuit_statistic: CircuitStatistic {
                main_vm: (MAX_CIRCUITS_PER_BATCH
                    - 1
                    - circuit_statistics_bootloader_batch_tip_overhead(
                        ProtocolVersionId::latest().into(),
                    )) as f32,
                ..CircuitStatistic::default()
            },
            ..VmExecutionMetrics::default()
        };

        test_include_and_seal_block_resolution(
            block_execution_metrics,
            &CircuitsCriterion,
            protocol_version,
        );

        let block_execution_metrics = VmExecutionMetrics {
            circuit_statistic: CircuitStatistic {
                main_vm: MAX_CIRCUITS_PER_BATCH as f32,
                ..CircuitStatistic::default()
            },
            ..VmExecutionMetrics::default()
        };

        test_exclude_and_seal_block_resolution(
            block_execution_metrics,
            &CircuitsCriterion,
            protocol_version,
        );

        let tx_execution_metrics = VmExecutionMetrics {
            circuit_statistic: CircuitStatistic {
                main_vm: MAX_CIRCUITS_PER_BATCH as f32
                    * config.reject_tx_at_geometry_percentage as f32
                    + 1.0,
                ..CircuitStatistic::default()
            },
            ..VmExecutionMetrics::default()
        };

        test_unexecutable_tx_resolution(tx_execution_metrics, &CircuitsCriterion, protocol_version);
    }
}
