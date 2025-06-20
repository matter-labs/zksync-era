use zksync_multivm::utils::get_bootloader_encoding_space;
use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{
    L1BatchSealConfig, SealCriterion, SealData, SealResolution, UnexecutableReason,
};

#[derive(Debug)]
pub struct TxEncodingSizeCriterion;

impl SealCriterion for TxEncodingSizeCriterion {
    fn should_seal(
        &self,
        config: &L1BatchSealConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version_id: ProtocolVersionId,
    ) -> SealResolution {
        let bootloader_tx_encoding_space =
            get_bootloader_encoding_space(protocol_version_id.into());

        let reject_bound =
            (bootloader_tx_encoding_space as f64 * config.reject_tx_at_geometry_percentage).round();
        let include_and_seal_bound = (bootloader_tx_encoding_space as f64
            * config.close_block_at_geometry_percentage)
            .round();

        if tx_data.cumulative_size > reject_bound as usize {
            UnexecutableReason::LargeEncodingSize.into()
        } else if block_data.cumulative_size > bootloader_tx_encoding_space as usize {
            SealResolution::ExcludeAndSeal
        } else if block_data.cumulative_size > include_and_seal_bound as usize {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn capacity_filled(
        &self,
        _config: &L1BatchSealConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        let used_size = block_data.cumulative_size as f64;
        let full_size = get_bootloader_encoding_space(protocol_version.into()) as f64;
        Some(used_size / full_size)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "tx_encoding_size"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_criterion() {
        let bootloader_tx_encoding_space =
            get_bootloader_encoding_space(ProtocolVersionId::latest().into());

        // Create an empty config and only setup fields relevant for the test.
        let config = L1BatchSealConfig {
            reject_tx_at_geometry_percentage: 0.95,
            close_block_at_geometry_percentage: 0.95,
            ..L1BatchSealConfig::for_tests()
        };

        let criterion = TxEncodingSizeCriterion;

        let empty_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        let unexecutable_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData::default(),
            &SealData {
                cumulative_size: bootloader_tx_encoding_space as usize + 1,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(
            unexecutable_resolution,
            UnexecutableReason::LargeEncodingSize.into()
        );

        let exclude_and_seal_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData {
                cumulative_size: bootloader_tx_encoding_space as usize + 1,
                ..SealData::default()
            },
            &SealData {
                cumulative_size: 1,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(exclude_and_seal_resolution, SealResolution::ExcludeAndSeal);

        let include_and_seal_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData {
                cumulative_size: bootloader_tx_encoding_space as usize,
                ..SealData::default()
            },
            &SealData {
                cumulative_size: 1,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(include_and_seal_resolution, SealResolution::IncludeAndSeal);
    }
}
