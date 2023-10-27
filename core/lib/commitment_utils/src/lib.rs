//! Utils for commitment calculation.
use zkevm_test_harness::witness::utils::{
    events_queue_commitment_fixed, initial_heap_content_commitment_fixed,
};
use zksync_types::{LogQuery, ProtocolVersionId, H256, U256, USED_BOOTLOADER_MEMORY_BYTES};
use zksync_utils::expand_memory_contents;

pub fn events_queue_commitment(
    events_queue: &Vec<LogQuery>,
    protocol_version: ProtocolVersionId,
) -> Option<H256> {
    match protocol_version {
        id if id < ProtocolVersionId::Version18 => None,
        ProtocolVersionId::Version18 => Some(H256(events_queue_commitment_fixed(events_queue))),
        id => unimplemented!("events_queue_commitment is not implemented for {id:?}"),
    }
}

pub fn bootloader_initial_content_commitment(
    initial_bootloader_contents: &[(usize, U256)],
    protocol_version: ProtocolVersionId,
) -> Option<H256> {
    match protocol_version {
        id if id < ProtocolVersionId::Version18 => None,
        ProtocolVersionId::Version18 => {
            let full_bootloader_memory =
                expand_memory_contents(initial_bootloader_contents, USED_BOOTLOADER_MEMORY_BYTES);
            Some(H256(initial_heap_content_commitment_fixed(
                &full_bootloader_memory,
            )))
        }
        id => unimplemented!("events_queue_commitment is not implemented for {id:?}"),
    }
}
