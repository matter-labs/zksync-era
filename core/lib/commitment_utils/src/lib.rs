//! Utils for commitment calculation.
use multivm::utils::get_used_bootloader_memory_bytes;
use zkevm_test_harness::witness::utils::{
    events_queue_commitment_fixed, initial_heap_content_commitment_fixed,
};
use zksync_types::{LogQuery, ProtocolVersionId, H256, U256};
use zksync_utils::expand_memory_contents;

pub fn events_queue_commitment(
    events_queue: &Vec<LogQuery>,
    protocol_version: ProtocolVersionId,
) -> Option<H256> {
    (!protocol_version.is_pre_boojum()).then(|| H256(events_queue_commitment_fixed(events_queue)))
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
    let commitment = H256(initial_heap_content_commitment_fixed(
        &full_bootloader_memory,
    ));

    Some(commitment)
}
