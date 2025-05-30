use zksync_types::StopToken;
use zksync_vm2::interface::{GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer};

use crate::interface::storage::{StoragePtr, WriteStorage};

/// Tracer that stops VM execution once too many storage slots were read from the underlying storage.
/// Useful as an anti-DoS measure.
#[derive(Debug)]
pub struct StorageInvocationsTracer<ST> {
    storage: Option<StoragePtr<ST>>,
    stop_token: Option<StopToken>,
    limit: usize,
}

impl<ST: WriteStorage> Default for StorageInvocationsTracer<ST> {
    fn default() -> Self {
        Self {
            storage: None,
            stop_token: None,
            limit: usize::MAX,
        }
    }
}

impl<ST: WriteStorage> StorageInvocationsTracer<ST> {
    pub fn new(storage: StoragePtr<ST>, limit: usize) -> Self {
        Self {
            storage: Some(storage),
            stop_token: None,
            limit,
        }
    }

    #[must_use]
    pub fn with_stop_token(mut self, stop_token: StopToken) -> Self {
        self.stop_token = Some(stop_token);
        self
    }
}

impl<ST: WriteStorage> Tracer for StorageInvocationsTracer<ST> {
    #[inline(always)]
    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
        &mut self,
        _state: &mut S,
    ) -> ShouldStop {
        if matches!(OP::VALUE, Opcode::StorageRead | Opcode::StorageWrite) {
            // **Important:** We only check `stop_token` after specific opcodes in order to not degrade VM performance.
            // It helps that it is specifically storage operations that can be slow.
            if self.stop_token.as_ref().is_some_and(StopToken::should_stop) {
                return ShouldStop::Stop;
            }

            if let Some(storage) = self.storage.as_ref() {
                if storage.borrow().missed_storage_invocations() >= self.limit {
                    return ShouldStop::Stop;
                }
            }
        }
        ShouldStop::Continue
    }
}
