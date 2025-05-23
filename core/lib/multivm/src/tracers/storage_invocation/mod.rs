use zksync_types::StopToken;

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
    limit: usize,
    current: usize,
    stop_token: Option<StopToken>,
}

impl StorageInvocations {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            current: 0,
            stop_token: None,
        }
    }

    #[must_use]
    pub fn with_stop_token(mut self, stop_token: StopToken) -> Self {
        self.stop_token = Some(stop_token);
        self
    }
}

impl IntoOldVmTracer for StorageInvocations {
    fn old_tracer(&self) -> OldTracers {
        OldTracers::StorageInvocations(self.limit)
    }
}
