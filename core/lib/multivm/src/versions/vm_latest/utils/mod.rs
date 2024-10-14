//! Utility functions for the VM.

use once_cell::sync::Lazy;
use zk_evm_1_5_0::aux_structures::MemoryPage;
use zksync_types::{H256, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_vm_interface::VmEvent;

pub mod fee;
pub mod l2_blocks;
pub(crate) mod logs;
pub mod overhead;
pub mod transaction_encoding;

pub const fn heap_page_from_base(base: MemoryPage) -> MemoryPage {
    MemoryPage(base.0 + 2)
}

/// Extracts all bytecodes marked as known on the system contracts.
pub fn extract_bytecodes_marked_as_known(all_generated_events: &[VmEvent]) -> Vec<H256> {
    static PUBLISHED_BYTECODE_SIGNATURE: Lazy<H256> = Lazy::new(|| {
        ethabi::long_signature(
            "MarkedAsKnown",
            &[ethabi::ParamType::FixedBytes(32), ethabi::ParamType::Bool],
        )
    });

    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == KNOWN_CODES_STORAGE_ADDRESS
                && event.indexed_topics.len() == 3
                && event.indexed_topics[0] == *PUBLISHED_BYTECODE_SIGNATURE
        })
        .map(|event| event.indexed_topics[1])
        .collect()
}
