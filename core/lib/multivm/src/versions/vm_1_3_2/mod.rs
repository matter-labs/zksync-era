#![allow(clippy::derive_partial_eq_without_eq)]

pub use zk_evm_1_3_3::{self, block_properties::BlockProperties};

pub(crate) use self::vm_instance::VmInstance;
pub use self::{
    errors::TxRevertReason,
    history_recorder::{HistoryDisabled, HistoryEnabled, HistoryMode},
    oracle_tools::OracleTools,
    oracles::storage::StorageOracle,
    vm::Vm,
};

mod bootloader_state;
pub mod errors;
pub mod event_sink;
mod events;
mod history_recorder;
pub mod memory;
mod oracle_tools;
pub mod oracles;
mod pubdata_utils;
mod refunds;
pub mod test_utils;
pub mod transaction_data;
pub mod utils;
mod vm;
pub mod vm_instance;
pub mod vm_with_bootloader;

pub type Word = zksync_types::U256;

pub const MEMORY_SIZE: usize = 1 << 16;
pub const MAX_CALLS: usize = 65536;
pub const REGISTERS_COUNT: usize = 16;
pub const MAX_STACK_SIZE: usize = 256;
pub const MAX_CYCLES_FOR_TX: u32 = u32::MAX;
