//!
//! The script that returns the L2 gas price constants is that calculates the constants currently used by the
//! bootloader as well as L1 smart contracts. It should be used to edit the config file located in the contracts/system-contracts/SystemConfig.json
//! as well as contracts/SystemConfig.json
//!

use multivm::utils::get_bootloader_encoding_space;
use zksync_types::{ethabi::Address, IntrinsicSystemGasConstants, ProtocolVersionId, U256};

use crate::utils::{
    execute_internal_transfer_test, execute_user_txs_in_test_gas_vm, get_l1_tx, get_l1_txs,
    get_l2_txs, metrics_from_txs, TransactionGenerator,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VmSpentResourcesResult {
    // How many gas was spent on computation.
    pub(crate) gas_consumed: u32,
    // How many bytes of public data was published.
    pub(crate) pubdata_published: u32,
    // The total amount of gas the users have paid for.
    pub(crate) total_gas_paid: u32,
    // The total amount of gas the users have paid for public data.
    // TODO (SMA-1698): make it an option, since sometimes its calculation is skipped.
    pub(crate) total_pubdata_paid: u32,
}

struct IntrinsicPrices {
    tx_intrinsic_gas: u32,
    tx_intrinsic_pubdata: u32,
    bootloader_intrinsic_gas: u32,
    bootloader_intrinsic_pubdata: u32,
}

pub(crate) fn l2_gas_constants() -> IntrinsicSystemGasConstants {
    let IntrinsicPrices {
        tx_intrinsic_gas: l2_tx_intrinsic_gas,
        tx_intrinsic_pubdata: l2_tx_intrinsic_pubdata,
        bootloader_intrinsic_gas,
        bootloader_intrinsic_pubdata,
    } = get_intrinsic_overheads_for_tx_type(&get_l2_txs);

    let IntrinsicPrices {
        tx_intrinsic_gas: l1_tx_intrinsic_gas,
        tx_intrinsic_pubdata: l1_tx_intrinsic_pubdata,
        bootloader_intrinsic_gas: bootloader_intrinsic_gas_with_l1_txs,
        bootloader_intrinsic_pubdata: bootloader_intrinsic_pubdata_with_l1_txs,
    } = get_intrinsic_overheads_for_tx_type(&get_l1_txs);

    // Sanity checks: the bootloader on its own should consume the same resources
    // independently on the type of the transaction
    assert_eq!(
        bootloader_intrinsic_gas,
        bootloader_intrinsic_gas_with_l1_txs
    );
    assert_eq!(
        bootloader_intrinsic_pubdata,
        bootloader_intrinsic_pubdata_with_l1_txs
    );

    // Getting the amount of gas that is required for a transfer assuming no
    // pubdata price (this will be the case for most L2 txs as refund usually will not
    // change any new storage slots).
    let l2_tx_gas_for_refund_transfer = execute_internal_transfer_test();

    // This is needed to get the minimum number of gas a transaction could require.
    let empty_l1_tx_result = execute_user_txs_in_test_gas_vm(
        vec![
            // Using 0 gas limit to make sure that only the mandatory parts of the L1->L2 transaction are executed.
            get_l1_tx(
                0,
                Address::random(),
                Address::random(),
                0,
                Some(U256::zero()),
                None,
                vec![],
            )
            .into(),
        ],
        true,
    );

    // This price does not include the overhead for the transaction itself, but rather auxiliary parts
    // that must be done by the transaction and it can not be enforced by the operator to not to accept
    // the transaction if it does not cover the minimal costs.
    let min_l1_tx_price = empty_l1_tx_result.gas_consumed - bootloader_intrinsic_gas;

    // The price for each keccak circuit is increased by 136 bytes at a time, while
    // the transaction's size can be only increased by 32 at a time. Thus, for measurements
    // we will use the LCM(136, 32) = 544 to measure the increase with the transaction's size.
    const DELTA_IN_TX_SIZE: usize = 544;

    let lengthier_tx_result = execute_user_txs_in_test_gas_vm(
        vec![get_l1_tx(
            0,
            Address::random(),
            Address::random(),
            0,
            Some(U256::zero()),
            Some(vec![0u8; DELTA_IN_TX_SIZE]),
            vec![],
        )
        .into()],
        true,
    );

    let delta_from_544_bytes = lengthier_tx_result.gas_consumed - empty_l1_tx_result.gas_consumed;

    // The number of public data per factory dependencies should not depend on the size/structure of the factory
    // dependency, since the dependency has already been published on L1.
    let tx_with_more_factory_deps_result = execute_user_txs_in_test_gas_vm(
        vec![get_l1_tx(
            0,
            Address::random(),
            Address::random(),
            0,
            Some(U256::zero()),
            None,
            vec![vec![0u8; 32]],
        )
        .into()],
        true,
    );

    let gas_delta_from_factory_dep =
        tx_with_more_factory_deps_result.gas_consumed - empty_l1_tx_result.gas_consumed;
    let pubdata_delta_from_factory_dep =
        tx_with_more_factory_deps_result.pubdata_published - empty_l1_tx_result.pubdata_published;

    // The number of the bootloader memory that can be filled up with transactions.
    let bootloader_tx_memory_size_slots =
        get_bootloader_encoding_space(ProtocolVersionId::latest().into());

    IntrinsicSystemGasConstants {
        l2_tx_intrinsic_gas,
        l2_tx_intrinsic_pubdata,
        l2_tx_gas_for_refund_transfer,
        l1_tx_intrinsic_gas,
        l1_tx_intrinsic_pubdata,
        l1_tx_delta_factory_dep_gas: gas_delta_from_factory_dep,
        l1_tx_delta_factory_dep_pubdata: pubdata_delta_from_factory_dep,
        l1_tx_delta_544_encoding_bytes: delta_from_544_bytes,
        l1_tx_min_gas_base: min_l1_tx_price,
        bootloader_intrinsic_gas,
        bootloader_intrinsic_pubdata,
        bootloader_tx_memory_size_slots,
    }
}

// Takes the consumed resources before the transaction's inclusion and with the transaction included
// and returns the pair of the intrinsic gas cost (for computation) and the intrinsic cost in pubdata
fn get_intrinsic_price(
    prev_result: VmSpentResourcesResult,
    new_result: VmSpentResourcesResult,
) -> (u32, u32) {
    let delta_consumed_gas = new_result.gas_consumed - prev_result.gas_consumed;
    let delta_paid_gas = new_result.total_gas_paid - prev_result.total_gas_paid;

    let overhead_gas = delta_consumed_gas.saturating_sub(delta_paid_gas);

    let delta_consumed_pubdata = new_result.pubdata_published - prev_result.pubdata_published;
    let delta_paid_pubdata = new_result.total_pubdata_paid - prev_result.total_pubdata_paid;

    let overhead_pubdata = delta_consumed_pubdata.saturating_sub(delta_paid_pubdata);

    (overhead_gas, overhead_pubdata)
}

fn get_intrinsic_overheads_for_tx_type(tx_generator: &TransactionGenerator) -> IntrinsicPrices {
    let result_0 = metrics_from_txs(0, tx_generator);
    let result_1 = metrics_from_txs(1, tx_generator);
    let result_2 = metrics_from_txs(2, tx_generator);
    let result_3 = metrics_from_txs(3, tx_generator);

    // Firstly, let's calculate the bootloader overhead in gas.
    // It is equal to the number of gas spent when there are no transactions in the block.
    let bootloader_intrinsic_gas = result_0.gas_consumed;
    // The same goes for the overhead for the bootloader in pubdata
    let bootloader_intrinsic_pubdata = result_0.pubdata_published;

    // For various small reasons the overhead for the first transaction and for all the subsequent ones
    // might differ a bit, so we will calculate both and will use the maximum one as the result for L2 txs.

    let (tx1_intrinsic_gas, tx1_intrinsic_pubdata) = get_intrinsic_price(result_0, result_1);
    let (tx2_intrinsic_gas, tx2_intrinsic_pubdata) = get_intrinsic_price(result_1, result_2);
    let (tx3_intrinsic_gas, tx3_intrinsic_pubdata) = get_intrinsic_price(result_2, result_3);

    // A sanity check: to make sure that the assumptions that are used
    // in this calculations hold: we expect the overheads from 2nd and 3rd transactions
    // (and intuitively with all the higher indices) to be the same.
    assert_eq!(
        tx2_intrinsic_gas, tx3_intrinsic_gas,
        "Overhead of 2nd and 3rd L2 transactions in gas are not equal"
    );
    assert_eq!(
        tx2_intrinsic_pubdata, tx3_intrinsic_pubdata,
        "Overhead of 2nd and 3rd L2 transactions in pubdata are not equal"
    );

    // Finally, the overhead for computation and pubdata are maxes for overheads for the L2 txs
    let tx_intrinsic_gas = std::cmp::max(tx1_intrinsic_gas, tx2_intrinsic_gas);
    let tx_intrinsic_pubdata = std::cmp::max(tx1_intrinsic_pubdata, tx2_intrinsic_pubdata);

    IntrinsicPrices {
        bootloader_intrinsic_gas,
        bootloader_intrinsic_pubdata,
        tx_intrinsic_gas,
        tx_intrinsic_pubdata,
    }
}
