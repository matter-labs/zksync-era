use zksync_types::ProtocolVersionId;

use crate::seal_criteria::{SealCriterion, SealData, SealResolution, StateKeeperConfig};

#[derive(Debug)]
pub(crate) struct L1L2TxsCriterion;

// With current gas consumption it's possible to execute 600 L1->L2 txs with 7500000 L1 gas.
const L1_L2_TX_COUNT_LIMIT: usize = 600;

impl SealCriterion for L1L2TxsCriterion {
    fn should_seal(
        &self,
        _config: &StateKeeperConfig,
        _tx_count: usize,
        l1_tx_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        _protocol_version_id: ProtocolVersionId,
    ) -> SealResolution {
        // With current gas consumption it's possible to execute 600 L1->L2 txs with 7500000 L1 gas.
        const L1_L2_TX_COUNT_LIMIT: usize = 600;

        // Gas usage of L1_L2_TX differs on Gateway and L1.
        // At the same time gas consumption of L1_L2_TX is the same per tx.
        // So we can use the same limit for L1 and Gateway, just choosing the one with the lowest number of txs.
        if l1_tx_count >= L1_L2_TX_COUNT_LIMIT {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn capacity_filled(
        &self,
        _config: &StateKeeperConfig,
        _tx_count: usize,
        l1_tx_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        let used_count = l1_tx_count as f64;
        let full_count = L1_L2_TX_COUNT_LIMIT as f64;

        Some(used_count / full_count)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "l1_l2_txs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l1_l2_txs_seal_criterion() {
        let max_single_tx_gas = 15_000_000;
        let close_block_at_gas_percentage = 0.95;

        let l1_tx_count_bound = 599;

        // Create an empty config and only setup fields relevant for the test.
        let config = StateKeeperConfig {
            max_single_tx_gas,
            close_block_at_gas_percentage,
            ..StateKeeperConfig::for_tests()
        };

        let criterion = L1L2TxsCriterion;

        // Empty block should fit into gas criterion.
        let empty_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        // `l1_tx_count_bound` should return `NoSeal`.
        let block_resolution = criterion.should_seal(
            &config,
            0,
            l1_tx_count_bound as usize,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(block_resolution, SealResolution::NoSeal);

        // `l1_tx_count_bound + 1` should return `IncludeAndSeal`.
        let block_resolution = criterion.should_seal(
            &config,
            0,
            l1_tx_count_bound as usize + 1,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(block_resolution, SealResolution::IncludeAndSeal);
    }
}
