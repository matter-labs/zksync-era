use vm_interface::Halt;
use zk_evm::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};
use zksync_state::{StoragePtr, WriteStorage};

use crate::bootloader_state::BootloaderState;
use crate::old_vm::history_recorder::HistoryMode;
use crate::old_vm::memory::SimpleMemory;
use crate::types::internals::ZkSyncVmState;
use crate::VmExecutionStopReason;
