use zksync_types::ProtocolVersionId;

use crate::{
    gas_tracker::new_block_gas_count,
    state_keeper::seal_criteria::{SealCriterion, SealData, SealResolution, StateKeeperConfig},
};

/// This is a temporary solution.
/// Instead of checking for gas it simply checks that the contracts'
/// bytecode is large enough.
/// Among all the data which will be published on-chain the contracts'
/// bytecode is by far the largest one and with high probability
/// the slots will run out before the other pubdata becomes too big
#[derive(Debug)]
pub(crate) struct GasCriterion;

impl SealCriterion for GasCriterion {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        _block_open_timestamp_ms: u128,
        _tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        _protocol_version_id: ProtocolVersionId,
    ) -> SealResolution {
        let tx_bound =
            (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round() as u32;
        let block_bound =
            (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage).round() as u32;

        if (tx_data.gas_count + new_block_gas_count()).any_field_greater_than(tx_bound) {
            SealResolution::Unexecutable("Transaction requires too much gas".into())
        } else if block_data
            .gas_count
            .any_field_greater_than(config.max_single_tx_gas)
        {
            SealResolution::ExcludeAndSeal
        } else if block_data.gas_count.any_field_greater_than(block_bound) {
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
    use zksync_types::block::BlockGasCount;

    use super::*;

    #[test]
    fn test_gas_seal_criterion() {
        // Create an empty config and only setup fields relevant for the test.
        let config = StateKeeperConfig {
            max_single_tx_gas: 6000000,
            reject_tx_at_gas_percentage: 0.95,
            close_block_at_gas_percentage: 0.95,
            ..Default::default()
        };

        let criterion = GasCriterion;

        // Empty block should fit into gas criterion.
        let empty_block_gas = new_block_gas_count();
        let empty_block_resolution = criterion.should_seal(
            &config,
            0,
            0,
            &SealData {
                gas_count: empty_block_gas,
                ..SealData::default()
            },
            &SealData::default(),
            ProtocolVersionId::latest(),
        );
        assert_eq!(empty_block_resolution, SealResolution::NoSeal);

        let tx_gas = BlockGasCount {
            commit: config.max_single_tx_gas + 1,
            prove: 0,
            execute: 0,
        };
        // Transaction that needs more gas than a block limit should be unexecutable.
        let huge_transaction_resolution = criterion.should_seal(
            &config,
            0,
            1,
            &SealData {
                gas_count: empty_block_gas + tx_gas,
                ..SealData::default()
            },
            &SealData {
                gas_count: tx_gas,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(
            huge_transaction_resolution,
            SealResolution::Unexecutable("Transaction requires too much gas".into())
        );

        // Check criterion workflow
        let reject_tx_bound =
            (config.max_single_tx_gas as f64 * config.reject_tx_at_gas_percentage).round() as u32;
        let tx_gas = BlockGasCount {
            commit: reject_tx_bound - empty_block_gas.commit,
            prove: reject_tx_bound - empty_block_gas.prove,
            execute: reject_tx_bound - empty_block_gas.execute,
        };
        let resolution_after_first_tx = criterion.should_seal(
            &config,
            0,
            1,
            &SealData {
                gas_count: empty_block_gas + tx_gas,
                ..SealData::default()
            },
            &SealData {
                gas_count: tx_gas,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(resolution_after_first_tx, SealResolution::NoSeal);

        let resolution_after_second_tx = criterion.should_seal(
            &config,
            0,
            2,
            &SealData {
                gas_count: empty_block_gas + tx_gas + tx_gas,
                ..SealData::default()
            },
            &SealData {
                gas_count: tx_gas,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(resolution_after_second_tx, SealResolution::ExcludeAndSeal);

        // Check criterion workflow
        let tx_gas = BlockGasCount {
            commit: reject_tx_bound - empty_block_gas.commit - 1,
            prove: reject_tx_bound - empty_block_gas.prove - 1,
            execute: reject_tx_bound - empty_block_gas.execute - 1,
        };
        let close_bound =
            (config.max_single_tx_gas as f64 * config.close_block_at_gas_percentage).round() as u32;
        let block_gas = BlockGasCount {
            commit: close_bound + 1,
            prove: close_bound + 1,
            execute: close_bound + 1,
        };
        let resolution_after_first_tx = criterion.should_seal(
            &config,
            0,
            1,
            &SealData {
                gas_count: block_gas,
                ..SealData::default()
            },
            &SealData {
                gas_count: tx_gas,
                ..SealData::default()
            },
            ProtocolVersionId::latest(),
        );
        assert_eq!(resolution_after_first_tx, SealResolution::IncludeAndSeal);
    }
}
