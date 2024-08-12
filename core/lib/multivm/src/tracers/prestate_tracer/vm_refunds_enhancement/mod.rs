use std::collections::HashMap;

use zk_evm_1_3_3::tracing::{BeforeExecutionData, VmLocalStateData};
use zksync_types::{StorageKey, H256, U256};

use super::{
    get_account_data, process_modified_storage_keys, process_result, PrestateTracer, State,
    StorageAccess,
};
use crate::{
    interface::storage::{StoragePtr, WriteStorage},
    tracers::dyn_tracers::vm_1_3_3::DynTracer,
    vm_refunds_enhancement::{BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState},
};

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for PrestateTracer {
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        if self.config.diff_mode {
            self.pre
                .extend(process_modified_storage_keys(self.pre.clone(), &storage));
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for PrestateTracer {
    fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: crate::interface::tracer::VmExecutionStopReason,
    ) {
        let modified_storage_keys = state.storage.storage.inner().get_modified_storage_keys();
        if self.config.diff_mode {
            self.post = modified_storage_keys
                .iter()
                .map(|k| get_account_data(k.0, state, &modified_storage_keys))
                .collect::<State>();
        } else {
            let read_keys: &HashMap<StorageKey, H256> = &state
                .storage
                .storage
                .inner()
                .get_ptr()
                .borrow()
                .read_storage_keys()
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect();
            let res = read_keys
                .iter()
                .map(|k| get_account_data(k.0, state, &modified_storage_keys))
                .collect::<State>();
            self.post = res;
        }
        process_result(&self.result, self.pre.clone(), self.post.clone());
    }
}

impl<S: WriteStorage, H: HistoryMode> StorageAccess for ZkSyncVmState<S, H> {
    fn read_from_storage(&self, key: &StorageKey) -> U256 {
        self.storage.storage.read_from_storage(key)
    }
}
