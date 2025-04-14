use zksync_types::{
    aggregated_operations::GasConsts, settlement::SettlementLayer, ProtocolVersionId,
};

use crate::seal_criteria::{SealCriterion, SealData, SealResolution, StateKeeperConfig};

#[derive(Debug)]
pub(crate) struct L1L2TxsCriterion;

impl SealCriterion for L1L2TxsCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        l1_tx_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        _protocol_version_id: ProtocolVersionId,
        settlement_layer: &SettlementLayer,
    ) -> SealResolution {
        let costs = GasConsts::execute_costs(settlement_layer.is_gateway());
        // With current gas consumption it's possible to execute 600 L1->L2 txs with 7500000 L1 gas.
        const L1_L2_TX_COUNT_LIMIT: usize = 600;

        let block_l1_gas_bound =
            (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage).round() as u32;
        let l1_gas = costs.base + (l1_tx_count as u32) * costs.per_l1_l2_tx;

        // We check not only gas against `block_l1_gas_bound` but also count against `L1_L2_TX_COUNT_LIMIT`.
        // It's required in case `max_single_tx_gas` is set to some high value for gateway,
        // then chain migrates to L1 and there is some batch with large number of L1->L2 txs that is not yet executed.
        // This check guarantees that it will be possible to execute such batch with reasonable gas limit on L1.
        if l1_gas >= block_l1_gas_bound || l1_tx_count >= L1_L2_TX_COUNT_LIMIT {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn prom_criterion_name(&self) -> &'static str {
        "gas"
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::SLChainId;

    use super::*;

    #[test]
    fn test_l1_l2_txs_seal_criterion() {
        let max_single_tx_gas = 15_000_000;
        let close_block_at_gas_percentage = 0.95;

        let sl_layer = SettlementLayer::L1(SLChainId(10));
        let costs = GasConsts::execute_costs(sl_layer.is_gateway());
        let gas_bound = (max_single_tx_gas as f64 * close_block_at_gas_percentage).round() as u32;
        let l1_tx_count_bound = (gas_bound - costs.base - 1) / costs.per_l1_l2_tx;
        let l1_tx_count_bound = l1_tx_count_bound.min(599);

        // Create an empty config and only setup fields relevant for the test.
        let config = StateKeeperConfig {
            max_single_tx_gas,
            close_block_at_gas_percentage,
            ..Default::default()
        };

        let criterion = L1L2TxsCriterion;

        // Empty block should fit into gas criterion.
        let empty_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            0,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
            &sl_layer,
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        // `l1_tx_count_bound` should return `NoSeal`.
        let block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            l1_tx_count_bound as usize,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
            &sl_layer,
        );
        assert_eq!(block_resolution, SealResolution::NoSeal);

        // `l1_tx_count_bound + 1` should return `IncludeAndSeal`.
        let block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            l1_tx_count_bound as usize + 1,
            &SealData::default(),
            &SealData::default(),
            ProtocolVersionId::latest(),
            &sl_layer,
        );
        assert_eq!(block_resolution, SealResolution::IncludeAndSeal);
    }
}
