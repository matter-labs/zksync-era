use zksync_config::configs::chain::StateKeeperConfig;

use crate::seal_criteria::conditional_criteria::{SealCriterion, SealData, SealResolution};

#[derive(Debug)]
pub struct PayloadSizeCriterion;

impl SealCriterion for PayloadSizeCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _tx_count: usize,
        _block_gas_limit: u64,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution {
        if tx_data.payload_encoding_size > config.l2_block_max_payload_size {
            SealResolution::Unexecutable(
                "tx.payload_encoding_size > block.payload_encoding_size_limit".to_owned(),
            )
        } else if block_data.payload_encoding_size + tx_data.payload_encoding_size
            > config.l2_block_max_payload_size
        {
            SealResolution::Seal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "payload_size"
    }
}
