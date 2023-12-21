use crate::vm_latest::constants::{
    BLOCK_OVERHEAD_GAS, BLOCK_OVERHEAD_L1_GAS, BLOCK_OVERHEAD_PUBDATA, BOOTLOADER_TX_ENCODING_SPACE,
};
use zk_evm_1_4_1::zkevm_opcode_defs::system_params::MAX_TX_ERGS_LIMIT;
use zksync_system_constants::{MAX_L2_TX_GAS_LIMIT, MAX_TXS_IN_BLOCK};
use zksync_types::l1::is_l1_tx_type;
use zksync_types::U256;
use zksync_utils::ceil_div_u256;

/// Derives the overhead for processing transactions in a block.
pub fn derive_overhead(
    l1_gas_price: u64,
    base_fee: u64,
    encoded_len: usize,
    coeficients: OverheadCoeficients,
) -> u32 {
    let base_fee = U256::from(base_fee);

    let overhead_for_batch_eth = block_overhead_eth(l1_gas_price);

    // todo: move into constants
    let slot_overhead_gas = 10000; //ceil_div_u256(overhead_for_batch_eth, (base_fee * MAX_TXS_IN_BLOCK));
                                   // todo: the 32 constant is for words -> byte conversion

    // todo: move into constants
    let memory_overhead_gas = 10;
    // let memory_overhead_gas = ceil_div_u256(
    //     overhead_for_batch_eth,
    //     (base_fee * BOOTLOADER_TX_ENCODING_SPACE * 32),
    // );

    vec![
        slot_overhead_gas,
        memory_overhead_gas * (encoded_len as u32),
    ]
    .into_iter()
    .max()
    .unwrap()
}

/// Contains the coeficients with which the overhead for transactions will be calculated.
/// All of the coeficients should be <= 1. There are here to provide a certain "discount" for normal transactions
/// at the risk of malicious transactions that may close the block prematurely.
/// IMPORTANT: to perform correct computations, `MAX_TX_ERGS_LIMIT / coeficients.ergs_limit_overhead_coeficient` MUST
/// result in an integer number
#[derive(Debug, Clone, Copy)]
pub struct OverheadCoeficients {
    slot_overhead_coeficient: f64,
    bootloader_memory_overhead_coeficient: f64,
}

impl OverheadCoeficients {
    // This method ensures that the parameters keep the required invariants
    fn new_checked(
        slot_overhead_coeficient: f64,
        bootloader_memory_overhead_coeficient: f64,
    ) -> Self {
        Self {
            slot_overhead_coeficient,
            bootloader_memory_overhead_coeficient,
        }
    }

    // L1->L2 do not receive any discounts
    fn new_l1() -> Self {
        OverheadCoeficients::new_checked(1.0, 1.)
    }

    fn new_l2() -> Self {
        OverheadCoeficients::new_checked(
            1.0,
            1.0,
            // // For L2 transactions we allow a certain default discount with regard to the number of ergs.
            // // Multiinstance circuits can in theory be spawned infinite times, while projected future limitations
            // // on gas per pubdata allow for roughly 800kk gas per L1 batch, so the rough trust "discount" on the proof's part
            // // to be paid by the users is 0.1.
            // 0.1,
        )
    }

    /// Return the coeficients for the given transaction type
    pub fn from_tx_type(tx_type: u8) -> Self {
        if is_l1_tx_type(tx_type) {
            Self::new_l1()
        } else {
            Self::new_l2()
        }
    }
}

/// This method returns the overhead for processing the block  
/// TODO: maybe remove this method
pub(crate) fn get_amortized_overhead(
    l1_gas_price: u64,
    base_fee: u64,
    encoded_len: usize,
    coeficients: OverheadCoeficients,
) -> u32 {
    derive_overhead(l1_gas_price, base_fee, encoded_len, coeficients)
}

// todo: maybe remove this function
// pub(crate) fn block_overhead_gas(gas_per_pubdata_byte: u32) -> u32 {
//     BLOCK_OVERHEAD_GAS + BLOCK_OVERHEAD_PUBDATA * gas_per_pubdata_byte
// }

pub(crate) fn block_overhead_eth(l1_gas_price: u64) -> U256 {
    let l1_costs = U256::from(BLOCK_OVERHEAD_L1_GAS) * U256::from(l1_gas_price);

    l1_costs
}

#[cfg(test)]
mod tests {

    // use super::*;

    // const TEST_L1_GAS_PRICE: u64 = 50_000_000_000;
    // const TEST_FAIR_L2_GAS_PRICE: u64 = 2_500_000_000;
    // const TEST_BASE_FEE: u64 = 2_500_000_000;

    // // This method returns the maximum block overhead that can be charged from the user based on the binary search approach
    // pub(crate) fn get_maximal_allowed_overhead_bin_search(
    //     total_gas_limit: u32,
    //     gas_per_pubdata_byte_limit: u32,
    //     encoded_len: usize,
    //     coeficients: OverheadCoeficients,
    // ) -> u32 {
    //     let mut left_bound = if MAX_TX_ERGS_LIMIT < total_gas_limit {
    //         total_gas_limit - MAX_TX_ERGS_LIMIT
    //     } else {
    //         0u32
    //     };
    //     // Safe cast: the gas_limit for a transaction can not be larger than 2^32
    //     let mut right_bound = total_gas_limit;

    //     // The closure returns whether a certain overhead would be accepted by the bootloader.
    //     // It is accepted if the derived overhead (i.e. the actual overhead that the user has to pay)
    //     // is >= than the overhead proposed by the operator.
    //     let is_overhead_accepted = |suggested_overhead: u32| {
    //         let derived_overhead = derive_overhead(
    //             TEST_L1_GAS_PRICE,
    //             TEST_FAIR_L2_GAS_PRICE,
    //             TEST_BASE_FEE,
    //             encoded_len,
    //             coeficients,
    //         );

    //         derived_overhead >= suggested_overhead
    //     };

    //     // In order to find the maximal allowed overhead we are doing binary search
    //     while left_bound + 1 < right_bound {
    //         let mid = (left_bound + right_bound) / 2;

    //         if is_overhead_accepted(mid) {
    //             left_bound = mid;
    //         } else {
    //             right_bound = mid;
    //         }
    //     }

    //     if is_overhead_accepted(right_bound) {
    //         right_bound
    //     } else {
    //         left_bound
    //     }
    // }

    // #[test]
    // fn test_correctness_for_efficient_overhead() {
    //     let test_params = |total_gas_limit: u32,
    //                        gas_per_pubdata: u32,
    //                        encoded_len: usize,
    //                        coeficients: OverheadCoeficients| {
    //         let result_by_efficient_search =
    //             get_amortized_overhead(
    //                 TEST_L1_GAS_PRICE,
    //                 TEST_FAIR_L2_GAS_PRICE,
    //                 TEST_BASE_FEE,
    //                 encoded_len,
    //                  coeficients
    //             );

    //         let result_by_binary_search = get_maximal_allowed_overhead_bin_search(
    //             total_gas_limit,
    //             gas_per_pubdata,
    //             encoded_len,
    //             coeficients,
    //         );

    //         assert_eq!(result_by_efficient_search, result_by_binary_search);
    //     };

    //     // Some arbitrary test
    //     test_params(60_000_000, 800, 2900, OverheadCoeficients::new_l2());

    //     // Very small parameters
    //     test_params(0, 1, 12, OverheadCoeficients::new_l2());

    //     // Relatively big parameters
    //     let max_tx_overhead = derive_overhead(
    //         MAX_TX_ERGS_LIMIT,
    //         5000,
    //         10000,
    //         OverheadCoeficients::new_l2(),
    //     );
    //     test_params(
    //         MAX_TX_ERGS_LIMIT + max_tx_overhead,
    //         5000,
    //         10000,
    //         OverheadCoeficients::new_l2(),
    //     );

    //     test_params(115432560, 800, 2900, OverheadCoeficients::new_l1());
    // }
}
