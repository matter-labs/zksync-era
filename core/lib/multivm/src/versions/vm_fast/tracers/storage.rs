use zksync_vm2::interface::{GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer};

use crate::interface::storage::{StoragePtr, WriteStorage};

/// Tracer that stops VM execution once too many storage slots were read from the underlying storage.
/// Useful as an anti-DoS measure.
#[derive(Debug)]
pub struct StorageInvocationsTracer<ST> {
    storage: Option<StoragePtr<ST>>,
    limit: usize,
}

impl<ST: WriteStorage> Default for StorageInvocationsTracer<ST> {
    fn default() -> Self {
        Self {
            storage: None,
            limit: usize::MAX,
        }
    }
}

impl<ST: WriteStorage> StorageInvocationsTracer<ST> {
    pub fn new(storage: StoragePtr<ST>, limit: usize) -> Self {
        Self {
            storage: Some(storage),
            limit,
        }
    }
}

impl<ST: WriteStorage> Tracer for StorageInvocationsTracer<ST> {
    #[inline(always)]
    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
        &mut self,
        _state: &mut S,
    ) -> ShouldStop {
        if matches!(OP::VALUE, Opcode::StorageRead | Opcode::StorageWrite) {
            if let Some(storage) = self.storage.as_ref() {
                if storage.borrow().missed_storage_invocations() >= self.limit {
                    return ShouldStop::Stop;
                }
            }
        }
        ShouldStop::Continue
    }
}
