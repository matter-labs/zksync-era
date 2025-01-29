use zksync_vm2::interface::{GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer};

use crate::interface::storage::{StoragePtr, WriteStorage};

#[derive(Debug)]
pub struct StorageInvocationsTracer<ST> {
    // FIXME: this is very awkward; logically, storage should be obtainable from `GlobalStateInterface` (but how?)
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
            let storage = self.storage.as_ref().unwrap();
            if storage.borrow().missed_storage_invocations() > self.limit {
                return ShouldStop::Stop;
            }
        }
        ShouldStop::Continue
    }
}
