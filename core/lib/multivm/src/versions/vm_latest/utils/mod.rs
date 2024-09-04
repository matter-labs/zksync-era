use ethabi;
use once_cell::sync::Lazy;
use zk_evm_1_4_1::sha2;
use zk_evm_1_5_0::{
    aux_structures::MemoryPage,
    zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32},
};
use zksync_types::{H256, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_vm_interface::VmEvent;

/// Utility functions for the VM.
pub mod fee;
pub mod l2_blocks;
pub(crate) mod logs;
pub mod overhead;
pub mod transaction_encoding;

/// TODO: maybe move to a different folder
pub(crate) fn hash_evm_bytecode(bytecode: &[u8]) -> H256 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let len = bytecode.len() as u16;
    hasher.update(bytecode);
    let result = hasher.finalize();

    let mut output = [0u8; 32];
    output[..].copy_from_slice(result.as_slice());
    output[0] = BlobSha256Format::VERSION_BYTE;
    output[1] = 0;
    output[2..4].copy_from_slice(&len.to_be_bytes());

    H256(output)
}

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
