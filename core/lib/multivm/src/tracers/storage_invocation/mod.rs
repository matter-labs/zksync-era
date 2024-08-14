use crate::{glue::tracers::IntoOldVmTracer, tracers::old::OldTracers};

pub mod vm_1_4_1;
pub mod vm_1_4_2;
pub mod vm_boojum_integration;
pub mod vm_latest;
pub mod vm_refunds_enhancement;
pub mod vm_virtual_blocks;

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

impl IntoOldVmTracer for StorageInvocations {
    fn old_tracer(&self) -> OldTracers {
        OldTracers::StorageInvocations(self.limit)
    }
}
