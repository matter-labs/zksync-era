use serde::{Deserialize, Serialize};
use zksync_utils::ceil_div;

use crate::{circuit::CircuitStatistic, U256};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "result")]
pub struct TransactionExecutionMetrics {
    pub initial_storage_writes: usize,
    pub repeated_storage_writes: usize,
    pub gas_used: usize,
    pub gas_remaining: u32,
    pub event_topics: u16,
    pub published_bytecode_bytes: usize,
    pub l2_l1_long_messages: usize,
    pub l2_l1_logs: usize,
    pub contracts_used: usize,
    pub contracts_deployed: u16,
    pub vm_events: usize,
    pub storage_logs: usize,
    // it's the sum of storage logs, vm events, l2->l1 logs,
    // and the number of precompile calls
    pub total_log_queries: usize,
    pub cycles_used: u32,
    pub computational_gas_used: u32,
    pub total_updated_values_size: usize,
    pub pubdata_published: u32,
    pub circuit_statistic: CircuitStatistic,
}

impl Default for TransactionExecutionMetrics {
    fn default() -> Self {
        Self {
            initial_storage_writes: 0,
            repeated_storage_writes: 0,
            gas_used: 0,
            gas_remaining: u32::MAX,
            event_topics: 0,
            published_bytecode_bytes: 0,
            l2_l1_long_messages: 0,
            l2_l1_logs: 0,
            contracts_used: 0,
            contracts_deployed: 0,
            vm_events: 0,
            storage_logs: 0,
            total_log_queries: 0,
            cycles_used: 0,
            computational_gas_used: 0,
            total_updated_values_size: 0,
            pubdata_published: 0,
            circuit_statistic: Default::default(),
        }
    }
}

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
    let dynamic_len = ceil_div(data_len, 32)
        + ceil_div(signature_len, 32)
        + ceil_div(paymaster_input_len, 32)
        + ceil_div(reserved_dynamic_len, 32)
        + factory_deps_len;

    BASE_LEN + dynamic_len as usize
}
