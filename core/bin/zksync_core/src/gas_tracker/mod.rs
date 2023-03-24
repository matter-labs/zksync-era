//! This module predicts L1 gas cost for the Commit/PublishProof/Execute operations.

use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::BlockGasCount,
    commitment::BlockWithMetadata,
    tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics},
    ExecuteTransactionCommon, Transaction,
};

use self::constants::*;
pub mod constants;

pub fn agg_block_base_cost(op: AggregatedActionType) -> u32 {
    match op {
        AggregatedActionType::CommitBlocks => AGGR_BLOCK_COMMIT_BASE_COST,
        AggregatedActionType::PublishProofBlocksOnchain => AGGR_BLOCK_PROVE_BASE_COST,
        AggregatedActionType::ExecuteBlocks => AGGR_BLOCK_EXECUTE_BASE_COST,
    }
}

pub fn block_base_cost(op: AggregatedActionType) -> u32 {
    match op {
        AggregatedActionType::CommitBlocks => BLOCK_COMMIT_BASE_COST,
        AggregatedActionType::PublishProofBlocksOnchain => BLOCK_PROVE_BASE_COST,
        AggregatedActionType::ExecuteBlocks => BLOCK_EXECUTE_BASE_COST,
    }
}

pub trait GasCost {
    fn base_cost(&self, op: AggregatedActionType) -> u32;
}

impl GasCost for Transaction {
    fn base_cost(&self, op: AggregatedActionType) -> u32 {
        match op {
            AggregatedActionType::CommitBlocks => EXECUTE_COMMIT_COST,
            AggregatedActionType::PublishProofBlocksOnchain => 0,
            AggregatedActionType::ExecuteBlocks => match self.common_data {
                ExecuteTransactionCommon::L2(_) => EXECUTE_EXECUTE_COST,
                ExecuteTransactionCommon::L1(_) => L1_OPERATION_EXECUTE_COST,
            },
        }
    }
}

fn additional_pubdata_commit_cost(execution_metrics: &ExecutionMetrics) -> u32 {
    (execution_metrics.size() as u32) * GAS_PER_BYTE
}

fn additional_writes_commit_cost(writes_metrics: &DeduplicatedWritesMetrics) -> u32 {
    (writes_metrics.size() as u32) * GAS_PER_BYTE
}

pub fn new_block_gas_count() -> BlockGasCount {
    BlockGasCount {
        commit: block_base_cost(AggregatedActionType::CommitBlocks),
        prove: block_base_cost(AggregatedActionType::PublishProofBlocksOnchain),
        execute: block_base_cost(AggregatedActionType::ExecuteBlocks),
    }
}

pub fn gas_count_from_tx_and_metrics(
    tx: &Transaction,
    execution_metrics: &ExecutionMetrics,
) -> BlockGasCount {
    let commit = tx.base_cost(AggregatedActionType::CommitBlocks)
        + additional_pubdata_commit_cost(execution_metrics);
    BlockGasCount {
        commit,
        prove: tx.base_cost(AggregatedActionType::PublishProofBlocksOnchain),
        execute: tx.base_cost(AggregatedActionType::ExecuteBlocks),
    }
}

pub fn gas_count_from_metrics(execution_metrics: &ExecutionMetrics) -> BlockGasCount {
    BlockGasCount {
        commit: additional_pubdata_commit_cost(execution_metrics),
        prove: 0,
        execute: 0,
    }
}

pub fn gas_count_from_writes(writes_metrics: &DeduplicatedWritesMetrics) -> BlockGasCount {
    BlockGasCount {
        commit: additional_writes_commit_cost(writes_metrics),
        prove: 0,
        execute: 0,
    }
}

pub fn commit_gas_count_for_block(block: &BlockWithMetadata) -> u32 {
    let base_cost = block_base_cost(AggregatedActionType::CommitBlocks);
    let additional_calldata_bytes = block.metadata.initial_writes_compressed.len() as u32
        + block.metadata.repeated_writes_compressed.len() as u32
        + block.metadata.l2_l1_messages_compressed.len() as u32
        + block
            .header
            .l2_to_l1_messages
            .iter()
            .map(|message| message.len() as u32)
            .sum::<u32>()
        + block
            .factory_deps
            .iter()
            .map(|factory_dep| factory_dep.len() as u32)
            .sum::<u32>();
    let additional_cost = additional_calldata_bytes * GAS_PER_BYTE;
    base_cost + additional_cost
}
