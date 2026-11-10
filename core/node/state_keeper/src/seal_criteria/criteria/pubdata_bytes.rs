use zksync_multivm::utils::execution_metrics_bootloader_batch_tip_overhead;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{
    SealCriteriaConfig, SealCriterion, SealData, SealResolution, UnexecutableReason,
};

/// Pubdata attributable to `data`, measured the same way for a transaction and for the batch
/// it goes into.
///
/// `pubdata_published` is the delta of the batch-wide pubdata counter over the execution the
/// data describes, so per-transaction values sum to the batch total and the two are directly
/// comparable. Deriving the size from `StorageDeduplication` metrics is a fallback for VM
/// versions before virtual blocks, which do not report the counter.
fn pubdata_size(data: &SealData, protocol_version: ProtocolVersionId) -> usize {
    if data.execution_metrics.pubdata_published == 0 {
        data.execution_metrics.size() + data.writes_metrics.size(protocol_version)
    } else {
        data.execution_metrics.pubdata_published as usize
    }
}

#[derive(Debug)]
pub struct PubDataBytesCriterion;

impl SealCriterion for PubDataBytesCriterion {
    fn should_seal(
        &self,
        config: &SealCriteriaConfig,
        tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let max_pubdata_per_l1_batch = config.max_pubdata_per_batch.0 as usize;
        let reject_bound =
            (max_pubdata_per_l1_batch as f64 * config.reject_tx_at_eth_params_percentage).round();
        let include_and_seal_bound =
            (max_pubdata_per_l1_batch as f64 * config.close_block_at_eth_params_percentage).round();

        // Both bounds below have to be read off the same ruler. Measuring the transaction with
        // `pubdata_published` while measuring the batch with the legacy `StorageDeduplication`
        // estimate let a transaction pass the reject bound and then overflow the batch bound on
        // its own -- the estimate runs about 2x the real pubdata for many one-byte L2->L1
        // messages, because it counts the ABI-padded `L1MessageSent` event data rather than the
        // message length.
        let block_size = pubdata_size(block_data, protocol_version);
        let tx_size = pubdata_size(tx_data, protocol_version);
        let batch_tip_overhead =
            execution_metrics_bootloader_batch_tip_overhead(protocol_version.into());
        // A transaction that is alone in the batch and still overflows it would overflow the
        // next, equally empty, batch in the same way. `ExcludeAndSeal` there would roll it back
        // into the mempool, where it keeps its place at the head of the queue and is picked
        // again, sealing empty batches for as long as it is there. The verdict is final instead,
        // as in `GasForBatchTipCriterion`.
        let is_tx_first = tx_count == 1;
        if tx_size + batch_tip_overhead > reject_bound as usize {
            UnexecutableReason::PubdataLimit.into()
        } else if block_size + batch_tip_overhead > max_pubdata_per_l1_batch {
            if is_tx_first {
                UnexecutableReason::PubdataLimit.into()
            } else {
                SealResolution::ExcludeAndSeal
            }
        } else if block_size + batch_tip_overhead > include_and_seal_bound as usize {
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
        let used_pubdata = (pubdata_size(block_data, protocol_version)
            + execution_metrics_bootloader_batch_tip_overhead(protocol_version.into()))
            as f64;
        let full_pubdata = config.max_pubdata_per_batch.0 as f64;

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
        let config = SealCriteriaConfig {
            reject_tx_at_eth_params_percentage: 0.95,
            close_block_at_eth_params_percentage: 0.95,
            max_pubdata_per_batch: 100000.into(),
            ..SealCriteriaConfig::for_tests()
        };

        let criterion = PubDataBytesCriterion;

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

    /// `VmExecutionMetrics` measured in the VM for a single transaction that emits 800 one-byte
    /// L2->L1 messages through `L1Messenger.sendToL1`. The legacy estimate makes this look like
    /// 150,785 bytes of pubdata; the VM counter puts the real figure at 74,401.
    fn many_small_l2_to_l1_messages() -> VmExecutionMetrics {
        VmExecutionMetrics {
            gas_used: 12_540_438,
            l2_l1_long_messages: 76_800,
            l2_to_l1_logs: 800,
            user_l2_to_l1_logs: 800,
            vm_events: 1_603,
            storage_logs: 12_107,
            total_log_queries: 23_347,
            cycles_used: 610_518,
            computational_gas_used: 12_540_438,
            pubdata_published: 74_401,
            ..VmExecutionMetrics::default()
        }
    }

    /// Deployed mainnet pubdata budget (`diamond_init_max_pubdata_per_batch`).
    fn mainnet_like_config() -> SealCriteriaConfig {
        SealCriteriaConfig {
            reject_tx_at_eth_params_percentage: 0.95,
            close_block_at_eth_params_percentage: 0.95,
            max_pubdata_per_batch: 120_000.into(),
            ..SealCriteriaConfig::for_tests()
        }
    }

    /// A transaction whose real pubdata fits the batch must not be excluded from an empty batch
    /// just because the legacy estimate over-counts it. Excluding it would roll it back into the
    /// mempool, from where it is selected again, sealing empty batches indefinitely.
    #[test]
    fn transaction_that_fits_is_not_excluded_from_an_empty_batch() {
        let config = mainnet_like_config();
        let data = SealData {
            execution_metrics: many_small_l2_to_l1_messages(),
            ..SealData::default()
        };

        let resolution = PubDataBytesCriterion.should_seal(
            &config,
            1, // first and only transaction: `block_data` and `tx_data` describe the same tx
            0,
            0,
            &data,
            &data,
            ProtocolVersionId::latest(),
        );
        assert_eq!(resolution, SealResolution::NoSeal);
    }

    /// The transaction and the batch are measured identically, so the reject bound (a fraction of
    /// the budget) is always reached before the batch bound (the whole budget). A first
    /// transaction that passes admission therefore can never overflow the batch on its own.
    #[test]
    fn admission_bound_is_reached_before_the_batch_bound() {
        let config = mainnet_like_config();
        let batch_tip_overhead =
            execution_metrics_bootloader_batch_tip_overhead(ProtocolVersionId::latest().into());

        for pubdata_published in [1, 10_000, 74_401, 100_000, 113_000, 118_000, 130_000] {
            let data = SealData {
                execution_metrics: VmExecutionMetrics {
                    pubdata_published,
                    ..many_small_l2_to_l1_messages()
                },
                ..SealData::default()
            };
            let resolution = PubDataBytesCriterion.should_seal(
                &config,
                1,
                0,
                0,
                &data,
                &data,
                ProtocolVersionId::latest(),
            );
            let admitted = !matches!(resolution, SealResolution::Unexecutable(_));
            let fits = pubdata_published as usize + batch_tip_overhead
                <= config.max_pubdata_per_batch.0 as usize;
            assert!(
                !admitted || fits,
                "pubdata_published={pubdata_published} was admitted but does not fit: {resolution:?}"
            );
            assert_ne!(
                resolution,
                SealResolution::ExcludeAndSeal,
                "pubdata_published={pubdata_published} excluded as the first tx in a batch"
            );
        }
    }

    /// A later transaction that overflows the batch is still excluded so the batch is sealed and
    /// the transaction retried with a full budget -- only the *first* one gets a final verdict.
    #[test]
    fn later_transaction_overflowing_the_batch_is_still_excluded() {
        let config = mainnet_like_config();
        let tx_data = SealData {
            execution_metrics: many_small_l2_to_l1_messages(),
            ..SealData::default()
        };
        let block_data = SealData {
            execution_metrics: VmExecutionMetrics {
                pubdata_published: 119_000,
                ..many_small_l2_to_l1_messages()
            },
            ..SealData::default()
        };

        let resolution = PubDataBytesCriterion.should_seal(
            &config,
            2, // second transaction in the batch
            0,
            0,
            &block_data,
            &tx_data,
            ProtocolVersionId::latest(),
        );
        assert_eq!(resolution, SealResolution::ExcludeAndSeal);
    }
}
