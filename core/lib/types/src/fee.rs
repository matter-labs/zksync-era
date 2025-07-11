use serde::{Deserialize, Serialize};

use crate::U256;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fee {
    /// The limit of gas that are to be spent on the actual transaction.
    pub gas_limit: U256,
    /// ZKsync version of EIP1559 maxFeePerGas.
    pub max_fee_per_gas: U256,
    /// ZKsync version of EIP1559 maxPriorityFeePerGas.
    pub max_priority_fee_per_gas: U256,
    /// The maximal gas per pubdata byte the user agrees to.
    pub gas_per_pubdata_limit: U256,
}

impl Fee {
    pub fn max_total_fee(&self) -> U256 {
        self.max_fee_per_gas * self.gas_limit
    }

    pub fn get_effective_gas_price(&self, block_base_fee_per_gas: U256) -> U256 {
        assert!(block_base_fee_per_gas <= self.max_fee_per_gas);
        assert!(self.max_priority_fee_per_gas <= self.max_fee_per_gas);

        // For now, we charge only for base fee.
        block_base_fee_per_gas
    }
}

/// This is just a comment to trigger CI to recompile and run tests
/// Returns how many slots would ABI-encoding of the transaction with such parameters take
pub fn encoding_len(
    data_len: u64,
    signature_len: u64,
    factory_deps_len: u64,
    paymaster_input_len: u64,
    reserved_dynamic_len: u64,
) -> usize {
    // The length assuming that all the dynamic fields are empty, i.e. it includes
    // encoding of fixed-length fields and the lengths of the dynamic fields + 1 0x20 starting symbol
    const BASE_LEN: usize = 1 + 19 + 5;

    // All of the fields are encoded as `bytes`, so their encoding takes ceil(len, 32) slots.
    // For factory deps we only provide hashes, which are encoded as an array of bytes32.
    let dynamic_len = data_len.div_ceil(32)
        + signature_len.div_ceil(32)
        + paymaster_input_len.div_ceil(32)
        + reserved_dynamic_len.div_ceil(32)
        + factory_deps_len;

    BASE_LEN + dynamic_len as usize
}
