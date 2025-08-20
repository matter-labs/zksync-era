use zksync_multivm::utils::execution_metrics_bootloader_batch_tip_overhead;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{
    SealCriterion, SealData, SealResolution, StateKeeperConfig, UnexecutableReason,
};

#[derive(Debug)]
pub struct PubDataBytesCriterion {
    /// This value changes based on the DA solution.
    /// If we use calldata, the limit is `128kb`
    /// If we use blobs then the value can be up to `252kb`, up to `126kb` will fill 1 blob,
    /// more than that will switch over to 2 blobs.
    pub max_pubdata_per_batch: u64,
}

impl SealCriterion for PubDataBytesCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let max_pubdata_per_l1_batch = self.max_pubdata_per_batch as usize;
        let reject_bound =
            (max_pubdata_per_l1_batch as f64 * config.reject_tx_at_eth_params_percentage).round();
        let include_and_seal_bound =
            (max_pubdata_per_l1_batch as f64 * config.close_block_at_eth_params_percentage).round();

        let block_size =
            block_data.execution_metrics.size() + block_data.writes_metrics.size(protocol_version);
        // For backward compatibility, we need to keep calculating the size of the pubdata based
        // `StorageDeduplication` metrics. All vm versions
        // after vm with virtual blocks will provide the size of the pubdata in the execution metrics.
        let tx_size = if tx_data.execution_metrics.pubdata_published == 0 {
            tx_data.execution_metrics.size() + tx_data.writes_metrics.size(protocol_version)
        } else {
            tx_data.execution_metrics.pubdata_published as usize
        };
        if tx_size + execution_metrics_bootloader_batch_tip_overhead(protocol_version.into())
            > reject_bound as usize
        {
            UnexecutableReason::PubdataLimit.into()
        } else if block_size
            + execution_metrics_bootloader_batch_tip_overhead(protocol_version.into())
            > max_pubdata_per_l1_batch
        {
            SealResolution::ExcludeAndSeal
        } else if block_size
            + execution_metrics_bootloader_batch_tip_overhead(protocol_version.into())
            > include_and_seal_bound as usize
        {
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
        _interop_roots_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        let used_pubdata = (block_data.execution_metrics.size()
            + block_data.writes_metrics.size(protocol_version)
            + execution_metrics_bootloader_batch_tip_overhead(protocol_version.into()))
            as f64;
        let full_pubdata = self.max_pubdata_per_batch as f64;

        Some(used_pubdata / full_pubdata)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "pub_data_size"
    }
}

#[cfg(test)]
mod tests {
    use zksync_multivm::interface::VmExecutionMetrics;

    use super::*;

    #[test]
    fn seal_criterion() {
        // Create an empty config and only setup fields relevant for the test.
        let config = StateKeeperConfig {
            reject_tx_at_eth_params_percentage: 0.95,
            close_block_at_eth_params_percentage: 0.95,
            max_pubdata_per_batch: 100000.into(),
            ..StateKeeperConfig::for_tests()
        };

        let criterion = PubDataBytesCriterion {
            max_pubdata_per_batch: 100000,
        };

        let block_execution_metrics = VmExecutionMetrics {
            l2_l1_long_messages: (config.max_pubdata_per_batch.0 as f64
                * config.close_block_at_eth_params_percentage
                - 1.0
                - execution_metrics_bootloader_batch_tip_overhead(
                    ProtocolVersionId::latest().into(),
                ) as f64)
                .round() as usize,
            ..VmExecutionMetrics::default()
        };

        let empty_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            0,
            &SealData {
                execution_metrics: block_execution_metrics,
                ..SealData::default()
            },
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        let block_execution_metrics = VmExecutionMetrics {
            l2_l1_long_messages: (config.max_pubdata_per_batch.0 as f64
                * config.close_block_at_eth_params_percentage
                + 1f64)
                .round() as usize,
            ..VmExecutionMetrics::default()
        };

        let full_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            0,
            &SealData {
                execution_metrics: block_execution_metrics,
                ..SealData::default()
            },
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);

        let block_execution_metrics = VmExecutionMetrics {
            l2_l1_long_messages: config.max_pubdata_per_batch.0 as usize + 1,
            ..VmExecutionMetrics::default()
        };
        let full_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            0,
            &SealData {
                execution_metrics: block_execution_metrics,
                ..SealData::default()
            },
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(full_block_resolution, SealResolution::ExcludeAndSeal);
    }
}
