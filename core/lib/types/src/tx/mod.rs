//! `transactions` is module that holds the essential information for every transaction.
//!
//! Since in ZKsync Era every operation can be executed either from the contract or rollup,
//! it makes more sense to define the contents of each transaction chain-agnostic, and extent this data
//! with metadata (such as fees and/or signatures) for L1 and L2 separately.

use std::fmt::Debug;

use zksync_basic_types::{Address, H256};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use self::tx_execution_info::TxExecutionStatus;
pub use self::{execute::Execute, tx_execution_info::ExecutionMetrics};
use crate::{vm_trace::Call, Transaction};

pub mod execute;
pub mod tx_execution_info;
pub use zksync_crypto_primitives as primitives;

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionExecutionResult {
    pub transaction: Transaction,
    pub hash: H256,
    pub execution_info: ExecutionMetrics,
    pub execution_status: TxExecutionStatus,
    pub refunded_gas: u64,
    pub operator_suggested_refund: u64,
    pub compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    pub call_traces: Vec<Call>,
    pub revert_reason: Option<String>,
}

impl TransactionExecutionResult {
    pub fn call_trace(&self) -> Option<Call> {
        if self.call_traces.is_empty() {
            None
        } else {
            Some(Call::new_high_level(
                self.transaction.gas_limit().as_u64(),
                self.transaction.gas_limit().as_u64() - self.refunded_gas,
                self.transaction.execute.value,
                self.transaction.execute.calldata.clone(),
                vec![],
                self.revert_reason.clone(),
                self.call_traces.clone(),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IncludedTxLocation {
    pub tx_hash: H256,
    pub tx_index_in_l2_block: u32,
    pub tx_initiator_address: Address,
}
