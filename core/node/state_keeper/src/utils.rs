use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::BlockGasCount,
    tx::{tx_execution_info::DeduplicatedWritesMetrics, ExecutionMetrics},
    ExecuteTransactionCommon, ProtocolVersionId, Transaction,
};
// TODO(QIT-32): Remove constants(except `L1_OPERATION_EXECUTE_COST`) and logic that use them
const L1_BATCH_COMMIT_BASE_COST: u32 = 31_000;
const L1_BATCH_PROVE_BASE_COST: u32 = 7_000;
const L1_BATCH_EXECUTE_BASE_COST: u32 = 30_000;

const EXECUTE_COMMIT_COST: u32 = 0;
const EXECUTE_EXECUTE_COST: u32 = 0;

const L1_OPERATION_EXECUTE_COST: u32 = 12_500;

const GAS_PER_BYTE: u32 = 18;

pub(super) fn l1_batch_base_cost(op: AggregatedActionType) -> u32 {
    match op {
        AggregatedActionType::Commit => L1_BATCH_COMMIT_BASE_COST,
        AggregatedActionType::PublishProofOnchain => L1_BATCH_PROVE_BASE_COST,
        AggregatedActionType::Execute => L1_BATCH_EXECUTE_BASE_COST,
    }
}

fn base_tx_cost(tx: &Transaction, op: AggregatedActionType) -> u32 {
    match op {
        AggregatedActionType::Commit => EXECUTE_COMMIT_COST,
        AggregatedActionType::PublishProofOnchain => 0,
        AggregatedActionType::Execute => match tx.common_data {
            ExecuteTransactionCommon::L1(_) => L1_OPERATION_EXECUTE_COST,
            ExecuteTransactionCommon::L2(_) => EXECUTE_EXECUTE_COST,
            ExecuteTransactionCommon::ProtocolUpgrade(_) => EXECUTE_EXECUTE_COST,
        },
    }
}

fn additional_pubdata_commit_cost(execution_metrics: &ExecutionMetrics) -> u32 {
    (execution_metrics.size() as u32) * GAS_PER_BYTE
}

fn additional_writes_commit_cost(
    writes_metrics: &DeduplicatedWritesMetrics,
    protocol_version: ProtocolVersionId,
) -> u32 {
    (writes_metrics.size(protocol_version) as u32) * GAS_PER_BYTE
}

pub(super) fn new_block_gas_count() -> BlockGasCount {
    BlockGasCount {
        commit: l1_batch_base_cost(AggregatedActionType::Commit),
        prove: l1_batch_base_cost(AggregatedActionType::PublishProofOnchain),
        execute: l1_batch_base_cost(AggregatedActionType::Execute),
    }
}

pub(super) fn gas_count_from_tx_and_metrics(
    tx: &Transaction,
    execution_metrics: &ExecutionMetrics,
) -> BlockGasCount {
    let commit = base_tx_cost(tx, AggregatedActionType::Commit)
        + additional_pubdata_commit_cost(execution_metrics);
    BlockGasCount {
        commit,
        prove: base_tx_cost(tx, AggregatedActionType::PublishProofOnchain),
        execute: base_tx_cost(tx, AggregatedActionType::Execute),
    }
}

pub(super) fn gas_count_from_metrics(execution_metrics: &ExecutionMetrics) -> BlockGasCount {
    BlockGasCount {
        commit: additional_pubdata_commit_cost(execution_metrics),
        prove: 0,
        execute: 0,
    }
}

pub(super) fn gas_count_from_writes(
    writes_metrics: &DeduplicatedWritesMetrics,
    protocol_version: ProtocolVersionId,
) -> BlockGasCount {
    BlockGasCount {
        commit: additional_writes_commit_cost(writes_metrics, protocol_version),
        prove: 0,
        execute: 0,
    }
}
