use zksync_config::configs::chain::StateKeeperConfig;

use crate::seal_criteria::conditional_criteria::{SealCriterion, SealData, SealResolution};

#[derive(Debug)]
pub struct TxCountCriterion;

impl SealCriterion for TxCountCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        tx_count: usize,
        _block_gas_limit: u64,
        _block_data: &SealData,
        _tx_data: &SealData,
    ) -> SealResolution {
        if tx_count >= config.transaction_slots {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "tx_count"
    }
}
