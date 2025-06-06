use zksync_types::{l2_to_l1_log::l2_to_l1_logs_tree_size, ProtocolVersionId};

use crate::seal_criteria::{
    SealCriterion, SealData, SealResolution, StateKeeperConfig, UnexecutableReason,
};

#[derive(Debug)]
pub(crate) struct L2L1LogsCriterion;

impl SealCriterion for L2L1LogsCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version_id: ProtocolVersionId,
    ) -> SealResolution {
        let max_allowed_logs = l2_to_l1_logs_tree_size(protocol_version_id);

        let reject_bound =
            (max_allowed_logs as f64 * config.reject_tx_at_geometry_percentage).round() as usize;
        let include_and_seal_bound =
            (max_allowed_logs as f64 * config.close_block_at_geometry_percentage).round() as usize;

        // Only user logs are included in the tree, so we consider only their number.
        if tx_data.execution_metrics.user_l2_to_l1_logs >= reject_bound {
            UnexecutableReason::TooMuchUserL2L1Logs.into()
        } else if block_data.execution_metrics.user_l2_to_l1_logs > max_allowed_logs {
            SealResolution::ExcludeAndSeal
        } else if block_data.execution_metrics.user_l2_to_l1_logs >= include_and_seal_bound {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn capacity_filled(
        &self,
        _config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        let used_logs = block_data.execution_metrics.user_l2_to_l1_logs as f64;
        let full_logs = l2_to_l1_logs_tree_size(protocol_version) as f64;

        Some(used_logs / full_logs)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "l2_l1_logs"
    }
}

#[cfg(test)]
mod tests {
    use test_casing::test_casing;
    use zksync_multivm::interface::VmExecutionMetrics;

    use super::*;

    fn query_criterion(
        config: &StateKeeperConfig,
        block_data_logs: usize,
        tx_data_logs: usize,
        protocol_version_id: ProtocolVersionId,
    ) -> SealResolution {
        L2L1LogsCriterion.should_seal(
            config,
            0,
            0,
            &SealData {
                execution_metrics: VmExecutionMetrics {
                    user_l2_to_l1_logs: block_data_logs,
                    ..VmExecutionMetrics::default()
                },
                ..SealData::default()
            },
            &SealData {
                execution_metrics: VmExecutionMetrics {
                    user_l2_to_l1_logs: tx_data_logs,
                    ..VmExecutionMetrics::default()
                },
                ..SealData::default()
            },
            protocol_version_id,
        )
    }

    #[test_casing(2, [ProtocolVersionId::Version25, ProtocolVersionId::Version27])]
    fn test_l2_l1_logs_seal_criterion(protocol_version: ProtocolVersionId) {
        let max_allowed_logs = l2_to_l1_logs_tree_size(protocol_version);
        let config = StateKeeperConfig {
            close_block_at_geometry_percentage: 0.95,
            reject_tx_at_geometry_percentage: 0.9,
            ..StateKeeperConfig::for_tests()
        };

        let reject_bound =
            (max_allowed_logs as f64 * config.reject_tx_at_geometry_percentage).round() as usize;
        let include_and_seal_bound =
            (max_allowed_logs as f64 * config.close_block_at_geometry_percentage).round() as usize;

        // not enough logs to seal
        let resolution = query_criterion(
            &config,
            reject_bound - 1,
            reject_bound - 1,
            protocol_version,
        );
        assert_eq!(resolution, SealResolution::NoSeal);

        // reject tx with huge number of logs
        let resolution = query_criterion(&config, reject_bound, reject_bound, protocol_version);
        assert_eq!(resolution, UnexecutableReason::TooMuchUserL2L1Logs.into());

        // enough logs to include and seal
        let resolution = query_criterion(&config, include_and_seal_bound, 1, protocol_version);
        assert_eq!(resolution, SealResolution::IncludeAndSeal);

        // not enough logs to exclude and seal
        let resolution = query_criterion(&config, max_allowed_logs, 1, protocol_version);
        assert_eq!(resolution, SealResolution::IncludeAndSeal);

        // enough logs to exclude and seal
        let resolution = query_criterion(&config, max_allowed_logs + 1, 1, protocol_version);
        assert_eq!(resolution, SealResolution::ExcludeAndSeal);
    }
}
