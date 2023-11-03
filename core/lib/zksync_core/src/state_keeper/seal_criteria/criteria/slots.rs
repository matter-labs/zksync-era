use zksync_types::ProtocolVersionId;

use crate::state_keeper::seal_criteria::{
    SealCriterion, SealData, SealResolution, StateKeeperConfig,
};

/// Checks whether we should seal the block because we've run out of transaction slots.
#[derive(Debug)]
pub struct SlotsCriterion;

impl SealCriterion for SlotsCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        tx_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        if tx_count >= config.transaction_slots {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
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
        let config = StateKeeperConfig {
            transaction_slots: 2,
            ..Default::default()
        };

        let criterion = SlotsCriterion;

        let almost_full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            config.transaction_slots - 1,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(almost_full_block_resolution, SealResolution::NoSeal);

        let full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            config.transaction_slots,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(full_block_resolution, SealResolution::IncludeAndSeal);
    }
}
