use zk_evm_1_4_1::sha2;
use zk_evm_1_5_0::{aux_structures::MemoryPage, zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32}};
use zksync_types::H256;

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
    output[..].copy_from_slice(&result.as_slice());
    output[0] = BlobSha256Format::VERSION_BYTE;
    output[1] = 0;
    output[2..4].copy_from_slice(&len.to_be_bytes());

    H256(output)
}

pub const fn heap_page_from_base(base: MemoryPage) -> MemoryPage {
    MemoryPage(base.0 + 2)
}
