pub mod vm_latest;
pub mod vm_virtual_blocks;

use crate::interface::dyn_tracers::vm_1_3_3::DynTracer;
use crate::vm_virtual_blocks::{
    ExecutionEndTracer, ExecutionProcessing, HistoryMode, VmTracer, ZkSyncVmState,
};
use zksync_state::WriteStorage;

/// Tracer responsible for calculating the number of storage invocations and
/// stopping the VM execution if the limit is reached.
#[derive(Debug, Default, Clone)]
pub struct StorageInvocations {
    pub limit: usize,
    pub current: usize,
}

impl StorageInvocations {
    pub fn new(limit: usize) -> Self {
        Self { limit, current: 0 }
    }
}
