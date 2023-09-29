use vm::vm_with_bootloader::BOOTLOADER_TX_ENCODING_SPACE;

use crate::state_keeper::seal_criteria::{
    SealCriterion, SealData, SealResolution, StateKeeperConfig,
};

#[derive(Debug)]
pub struct TxEncodingSizeCriterion;

impl SealCriterion for TxEncodingSizeCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution {
        let reject_bound =
            (BOOTLOADER_TX_ENCODING_SPACE as f64 * config.reject_tx_at_geometry_percentage).round();
        let include_and_seal_bound = (BOOTLOADER_TX_ENCODING_SPACE as f64
            * config.close_block_at_geometry_percentage)
            .round();

        if tx_data.cumulative_size > reject_bound as usize {
            let message = "Transaction cannot be included due to large encoding size";
            SealResolution::Unexecutable(message.into())
        } else if block_data.cumulative_size > BOOTLOADER_TX_ENCODING_SPACE as usize {
            SealResolution::ExcludeAndSeal
        } else if block_data.cumulative_size > include_and_seal_bound as usize {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
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
        let config = StateKeeperConfig::from_env();
        let criterion = TxEncodingSizeCriterion;

        let empty_block_resolution =
            criterion.should_seal(&config, 0, 0, &SealData::default(), &SealData::default());
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        let unexecutable_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData::default(),
            &SealData {
                cumulative_size: BOOTLOADER_TX_ENCODING_SPACE as usize + 1,
                ..SealData::default()
            },
        );
        assert_eq!(
            unexecutable_resolution,
            SealResolution::Unexecutable(
                "Transaction cannot be included due to large encoding size".into()
            )
        );

        let exclude_and_seal_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData {
                cumulative_size: BOOTLOADER_TX_ENCODING_SPACE as usize + 1,
                ..SealData::default()
            },
            &SealData {
                cumulative_size: 1,
                ..SealData::default()
            },
        );
        assert_eq!(exclude_and_seal_resolution, SealResolution::ExcludeAndSeal);

        let include_and_seal_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData {
                cumulative_size: BOOTLOADER_TX_ENCODING_SPACE as usize,
                ..SealData::default()
            },
            &SealData {
                cumulative_size: 1,
                ..SealData::default()
            },
        );
        assert_eq!(include_and_seal_resolution, SealResolution::IncludeAndSeal);
    }
}
