use zksync_multivm::utils::get_bootloader_max_interop_roots_in_batch;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{SealCriterion, SealData, SealResolution, StateKeeperConfig};

/// Checks whether we should seal the block because we've run out of transaction slots.
#[derive(Debug)]
pub struct InteropRootsCriterion;

impl SealCriterion for InteropRootsCriterion {
    fn should_seal(
        &self,
        _config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        interop_roots_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        if protocol_version.is_pre_interop_fast_blocks() {
            // In pre-interop fast blocks, we don't have interop roots, so we never seal based on them.
            return SealResolution::NoSeal;
        }
        let max_interop_roots_in_batch =
            get_bootloader_max_interop_roots_in_batch(protocol_version.into());
        assert!(
            interop_roots_count <= max_interop_roots_in_batch,
            "Interop roots count ({}) must be lower than the bootloader constant MAX_MSG_ROOTS_IN_BATCH={} for protocol version {}",
            interop_roots_count, max_interop_roots_in_batch, protocol_version as u16
        );

        match interop_roots_count.cmp(&max_interop_roots_in_batch) {
            std::cmp::Ordering::Greater => SealResolution::ExcludeAndSeal,
            std::cmp::Ordering::Equal => SealResolution::IncludeAndSeal,
            std::cmp::Ordering::Less => SealResolution::NoSeal,
        }
    }

    fn capacity_filled(
        &self,
        _config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        interop_roots_count: usize,
        _block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        let max_interop_roots_in_batch =
            get_bootloader_max_interop_roots_in_batch(protocol_version.into());
        Some((interop_roots_count as f64) / (max_interop_roots_in_batch as f64))
    }

    fn prom_criterion_name(&self) -> &'static str {
        "interop_roots"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interop_roots_seal_criterion() {
        // Create an empty config and only setup fields relevant for the test.
        let config = StateKeeperConfig::for_tests();
        let max_interop_roots_in_batch =
            get_bootloader_max_interop_roots_in_batch(ProtocolVersionId::latest().into());

        let criterion = InteropRootsCriterion;

        let almost_full_block_resolution = criterion.should_seal(
            &config,
            1,
            0,
            max_interop_roots_in_batch - 1,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(almost_full_block_resolution, SealResolution::NoSeal);

        let full_block_resolution = criterion.should_seal(
            &config,
            1,
            0,
            max_interop_roots_in_batch,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);
    }
}
