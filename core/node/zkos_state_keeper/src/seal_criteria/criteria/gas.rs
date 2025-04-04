use zksync_config::configs::chain::StateKeeperConfig;

use crate::seal_criteria::conditional_criteria::{SealCriterion, SealData, SealResolution};

#[derive(Debug)]
pub struct GasCriterion;

impl SealCriterion for GasCriterion {
    fn should_seal(
        &self,
        _config: &StateKeeperConfig,
        _tx_count: usize,
        block_gas_limit: u64,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution {
        if tx_data.gas > block_gas_limit {
            SealResolution::Unexecutable("tx.gas_limit > block.gas_limit".to_owned())
        } else if block_data.gas + tx_data.gas > block_gas_limit {
            SealResolution::Seal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "gas"
    }
}
