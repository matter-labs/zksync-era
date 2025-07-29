use zksync_multivm::utils::get_bootloader_max_msg_roots_in_batch;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{SealCriterion, SealData, SealResolution, StateKeeperConfig};

/// Checks whether we should seal the block because we've run out of transaction slots.
#[derive(Debug)]
pub struct InteropRootsCriterion;

impl SealCriterion for InteropRootsCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        interop_roots_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let max_interop_roots_in_batch =
            get_bootloader_max_msg_roots_in_batch(protocol_version.into());
        assert!(
            interop_roots_count <= max_interop_roots_in_batch,
            "Interop roots count ({}) must be lower than the bootloader constant MAX_MSG_ROOTS_IN_BATCH={} for protocol version {}",
            interop_roots_count, max_interop_roots_in_batch, protocol_version as u16
        );

        if interop_roots_count >= config.interop_roots {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn capacity_filled(
        &self,
        config: &StateKeeperConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        interop_roots_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        Some((interop_roots_count as f64) / (config.interop_roots as f64))
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
        let config = StateKeeperConfig {
            interop_roots: 2,
            ..StateKeeperConfig::for_tests()
        };

        let criterion = InteropRootsCriterion;

        let almost_full_block_resolution = criterion.should_seal(
            &config,
            1,
            0,
            config.interop_roots - 1,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(almost_full_block_resolution, SealResolution::NoSeal);

        let full_block_resolution = criterion.should_seal(
            &config,
            1,
            0,
            config.interop_roots,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);
    }
}
