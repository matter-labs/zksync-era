use std::cmp::Ordering;

use zksync_multivm::utils::get_bootloader_max_txs_in_batch;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{L1BatchSealConfig, SealCriterion, SealData, SealResolution};

/// Checks whether we should seal the block because we've run out of transaction slots.
#[derive(Debug)]
pub struct SlotsCriterion;

impl SealCriterion for SlotsCriterion {
    fn should_seal(
        &self,
        config: &L1BatchSealConfig,
        tx_count: usize,
        _l1_tx_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let max_txs_in_batch = get_bootloader_max_txs_in_batch(protocol_version.into());
        assert!(
            config.transaction_slots <= max_txs_in_batch,
            "Configured transaction_slots ({}) must be lower than the bootloader constant MAX_TXS_IN_BLOCK={} for protocol version {}",
            config.transaction_slots, max_txs_in_batch, protocol_version as u16
        );

        match tx_count.cmp(&config.transaction_slots) {
            Ordering::Greater => SealResolution::ExcludeAndSeal,
            Ordering::Equal => SealResolution::IncludeAndSeal,
            Ordering::Less => SealResolution::NoSeal,
        }
    }

    fn capacity_filled(
        &self,
        config: &L1BatchSealConfig,
        tx_count: usize,
        _l1_tx_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        Some((tx_count as f64) / (config.transaction_slots as f64))
    }

    fn prom_criterion_name(&self) -> &'static str {
        "slots"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slots_seal_criterion() {
        // Create an empty config and only setup fields relevant for the test.
        let config = L1BatchSealConfig {
            transaction_slots: 2,
            ..L1BatchSealConfig::for_tests()
        };

        let criterion = SlotsCriterion;

        let almost_full_block_resolution = criterion.should_seal(
            &config,
            config.transaction_slots - 1,
            0,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(almost_full_block_resolution, SealResolution::NoSeal);

        let full_block_resolution = criterion.should_seal(
            &config,
            config.transaction_slots,
            0,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);
    }
}
