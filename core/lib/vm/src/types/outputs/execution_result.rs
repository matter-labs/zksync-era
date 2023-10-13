use vm_interface::VmExecutionResultAndLogs;
use zksync_config::constants::PUBLISH_BYTECODE_OVERHEAD;
use zksync_types::event::{extract_long_l2_to_l1_messages, extract_published_bytecodes};
use zksync_types::tx::tx_execution_info::VmExecutionLogs;
use zksync_types::tx::ExecutionMetrics;
use zksync_types::Transaction;
use zksync_utils::bytecode::bytecode_len_in_bytes;

pub fn get_execution_metrics(
    result: &VmExecutionResultAndLogs,
    tx: Option<&Transaction>,
) -> ExecutionMetrics {
    let contracts_deployed = tx
        .map(|tx| {
            tx.execute
                .factory_deps
                .as_ref()
                .map_or(0, |deps| deps.len() as u16)
        })
        .unwrap_or(0);

    // We published the data as ABI-encoded `bytes`, so the total length is:
    // - message length in bytes, rounded up to a multiple of 32
    // - 32 bytes of encoded offset
    // - 32 bytes of encoded length
    let l2_l1_long_messages = extract_long_l2_to_l1_messages(&result.logs.events)
        .iter()
        .map(|event| (event.len() + 31) / 32 * 32 + 64)
        .sum();

    let published_bytecode_bytes = extract_published_bytecodes(&result.logs.events)
        .iter()
        .map(|bytecodehash| {
            bytecode_len_in_bytes(*bytecodehash) + PUBLISH_BYTECODE_OVERHEAD as usize
        })
        .sum();

    ExecutionMetrics {
        gas_used: result.statistics.gas_used as usize,
        published_bytecode_bytes,
        l2_l1_long_messages,
        l2_l1_logs: result.logs.l2_to_l1_logs.len(),
        contracts_used: result.statistics.contracts_used,
        contracts_deployed,
        vm_events: result.logs.events.len(),
        storage_logs: result.logs.storage_logs.len(),
        total_log_queries: result.statistics.total_log_queries,
        cycles_used: result.statistics.cycles_used,
        computational_gas_used: result.statistics.computational_gas_used,
    }
}
