//! Utility functions for the VM.

use zk_evm_1_5_0::aux_structures::MemoryPage;

pub mod fee;
pub mod l2_blocks;
pub(crate) mod logs;
pub mod overhead;
pub mod transaction_encoding;

pub const fn heap_page_from_base(base: MemoryPage) -> MemoryPage {
    MemoryPage(base.0 + 2)
}
