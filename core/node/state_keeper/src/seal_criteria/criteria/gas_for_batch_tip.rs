use multivm::utils::gas_bootloader_batch_tip_overhead;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{SealCriterion, SealData, SealResolution, StateKeeperConfig};

/// Checks whether we should exclude the transaction because we don't have enough gas for batch tip.
#[derive(Debug)]
pub struct GasForBatchTipCriterion;

impl SealCriterion for GasForBatchTipCriterion {
    fn should_seal(
        &self,
        _config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        tx_count: usize,
        _block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let batch_tip_overhead = gas_bootloader_batch_tip_overhead(protocol_version.into());
        let is_tx_first = tx_count == 1;

        if tx_data.gas_remaining < batch_tip_overhead {
            if is_tx_first {
                SealResolution::Unexecutable("not_enough_gas_for_batch_tip".to_string())
            } else {
                SealResolution::ExcludeAndSeal
            }
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "gas_for_batch_tip"
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_gas_for_batch_tip_seal_criterion() {
        // Create an empty config.
        let config = StateKeeperConfig::default();

        let criterion = GasForBatchTipCriterion;
        let protocol_version = ProtocolVersionId::latest();

        let seal_data = SealData {
            gas_remaining: gas_bootloader_batch_tip_overhead(protocol_version.into()),
            ..Default::default()
        };
        let almost_full_block_resolution = criterion.should_seal(
            &config,
            Default::default(),
            1,
            &seal_data,
            &seal_data,
            protocol_version,
        );
        assert_eq!(almost_full_block_resolution, SealResolution::NoSeal);

        let seal_data = SealData {
            gas_remaining: gas_bootloader_batch_tip_overhead(protocol_version.into()) - 1,
            ..Default::default()
        };
        let full_block_first_tx_resolution = criterion.should_seal(
            &config,
            Default::default(),
            1,
            &seal_data,
            &seal_data,
            protocol_version,
        );
        assert_matches!(
            full_block_first_tx_resolution,
            SealResolution::Unexecutable(_)
        );

        let full_block_second_tx_resolution = criterion.should_seal(
            &config,
            Default::default(),
            2,
            &seal_data,
            &seal_data,
            protocol_version,
        );
        assert_eq!(
            full_block_second_tx_resolution,
            SealResolution::ExcludeAndSeal
        );
    }
}
