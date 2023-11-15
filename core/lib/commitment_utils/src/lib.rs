//! Utils for commitment calculation.
use zkevm_test_harness::witness::utils::{
    events_queue_commitment_fixed, initial_heap_content_commitment_fixed,
};
use zksync_types::{LogQuery, H256, U256, USED_BOOTLOADER_MEMORY_BYTES};
use zksync_utils::expand_memory_contents;

pub fn events_queue_commitment(events_queue: &Vec<LogQuery>, is_pre_boojum: bool) -> Option<H256> {
    (!is_pre_boojum).then(|| H256(events_queue_commitment_fixed(events_queue)))
}

pub fn bootloader_initial_content_commitment(
    initial_bootloader_contents: &[(usize, U256)],
    is_pre_boojum: bool,
) -> Option<H256> {
    (!is_pre_boojum).then(|| {
        let full_bootloader_memory =
            expand_memory_contents(initial_bootloader_contents, USED_BOOTLOADER_MEMORY_BYTES);
        H256(initial_heap_content_commitment_fixed(
            &full_bootloader_memory,
        ))
    })
}
