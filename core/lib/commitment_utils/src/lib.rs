//! Utils for commitment calculation.
use multivm::utils::get_used_bootloader_memory_bytes;
use zksync_types::{zk_evm_types::LogQuery, ProtocolVersionId, VmVersion, H256, U256};
use zksync_utils::expand_memory_contents;

pub fn events_queue_commitment(
    events_queue: &Vec<LogQuery>,
    protocol_version: ProtocolVersionId,
) -> Option<H256> {
    match VmVersion::from(protocol_version) {
        VmVersion::VmBoojumIntegration => Some(H256(
            zkevm_test_harness_1_4_0::witness::utils::events_queue_commitment_fixed(
                &events_queue.iter().map(|x| (*x).into()).collect(),
            ),
        )),
        VmVersion::Vm1_4_1 => Some(H256(
            zkevm_test_harness_1_4_1::witness::utils::events_queue_commitment_fixed(
                &events_queue.iter().map(|x| (*x).into()).collect(),
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
        VmVersion::Vm1_4_1 => Some(H256(
            zkevm_test_harness_1_4_1::witness::utils::initial_heap_content_commitment_fixed(
                &full_bootloader_memory,
            ),
        )),
        _ => unreachable!(),
    }
}
