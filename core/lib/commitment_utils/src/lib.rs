//! Utils for commitment calculation.
use multivm::utils::get_used_bootloader_memory_bytes;
use zk_evm_1_3_3::{
    aux_structures::Timestamp as Timestamp_1_3_3,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_3_3,
};
use zk_evm_1_4_1::{
    aux_structures::Timestamp as Timestamp_1_4_1,
    zk_evm_abstractions::queries::LogQuery as LogQuery_1_4_1,
};
use zksync_types::{zk_evm_types::LogQuery, ProtocolVersionId, VmVersion, H256, U256};
use zksync_utils::expand_memory_contents;

pub fn events_queue_commitment(
    events_queue: &[LogQuery],
    protocol_version: ProtocolVersionId,
) -> Option<H256> {
    match VmVersion::from(protocol_version) {
        VmVersion::VmBoojumIntegration => Some(H256(
            zkevm_test_harness_1_4_0::witness::utils::events_queue_commitment_fixed(
                &events_queue
                    .iter()
                    .map(|x| to_log_query_1_3_3(*x))
                    .collect(),
            ),
        )),
        VmVersion::Vm1_4_1 | VmVersion::Vm1_4_2 => Some(H256(
            zkevm_test_harness_1_4_1::witness::utils::events_queue_commitment_fixed(
                &events_queue
                    .iter()
                    .map(|x| to_log_query_1_4_1(*x))
                    .collect(),
            ),
        )),
        _ => None,
    }
}

pub fn bootloader_initial_content_commitment(
    initial_bootloader_contents: &[(usize, U256)],
    protocol_version: ProtocolVersionId,
) -> Option<H256> {
    let expanded_memory_size = if protocol_version.is_pre_boojum() {
        return None;
    } else {
        get_used_bootloader_memory_bytes(protocol_version.into())
    };

    let full_bootloader_memory =
        expand_memory_contents(initial_bootloader_contents, expanded_memory_size);

    match VmVersion::from(protocol_version) {
        VmVersion::VmBoojumIntegration => Some(H256(
            zkevm_test_harness_1_4_0::witness::utils::initial_heap_content_commitment_fixed(
                &full_bootloader_memory,
            ),
        )),
        VmVersion::Vm1_4_1 | VmVersion::Vm1_4_2 => Some(H256(
            zkevm_test_harness_1_4_1::witness::utils::initial_heap_content_commitment_fixed(
                &full_bootloader_memory,
            ),
        )),
        _ => unreachable!(),
    }
}

fn to_log_query_1_3_3(log_query: LogQuery) -> LogQuery_1_3_3 {
    LogQuery_1_3_3 {
        timestamp: Timestamp_1_3_3(log_query.timestamp.0),
        tx_number_in_block: log_query.tx_number_in_block,
        aux_byte: log_query.aux_byte,
        shard_id: log_query.shard_id,
        address: log_query.address,
        key: log_query.key,
        read_value: log_query.read_value,
        written_value: log_query.written_value,
        rw_flag: log_query.rw_flag,
        rollback: log_query.rollback,
        is_service: log_query.is_service,
    }
}

fn to_log_query_1_4_1(log_query: LogQuery) -> LogQuery_1_4_1 {
    LogQuery_1_4_1 {
        timestamp: Timestamp_1_4_1(log_query.timestamp.0),
        tx_number_in_block: log_query.tx_number_in_block,
        aux_byte: log_query.aux_byte,
        shard_id: log_query.shard_id,
        address: log_query.address,
        key: log_query.key,
        read_value: log_query.read_value,
        written_value: log_query.written_value,
        rw_flag: log_query.rw_flag,
        rollback: log_query.rollback,
        is_service: log_query.is_service,
    }
}
