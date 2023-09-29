use std::collections::HashMap;

use zk_evm::aux_structures::Timestamp;
use zk_evm::vm_state::VmLocalState;
use zksync_state::WriteStorage;

use zksync_types::{StorageKey, StorageLogQuery, StorageValue, U256};

use crate::old_vm::event_sink::InMemoryEventSink;
use crate::rollback::history_recorder::HistoryRecorder;
use crate::rollback::reversable_log::ReversableLog;
use crate::{SimpleMemory, Vm};

#[derive(Clone, Debug)]
pub(crate) struct ModifiedKeysMap(HashMap<StorageKey, StorageValue>);

// We consider hashmaps to be equal even if there is a key
// that is not present in one but has zero value in another.
impl PartialEq for ModifiedKeysMap {
    fn eq(&self, other: &Self) -> bool {
        for (key, value) in self.0.iter() {
            if *value != other.0.get(key).cloned().unwrap_or_default() {
                return false;
            }
        }
        for (key, value) in other.0.iter() {
            if *value != self.0.get(key).cloned().unwrap_or_default() {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct DecommitterTestInnerState {
    /// There is no way to "trully" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub(crate) modified_storage_keys: ModifiedKeysMap,
    pub(crate) known_bytecodes: HistoryRecorder<HashMap<U256, Vec<U256>>>,
    pub(crate) decommitted_code_hashes: HistoryRecorder<HashMap<U256, u32>>,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct StorageOracleInnerState {
    /// There is no way to "trully" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub(crate) modified_storage_keys: ModifiedKeysMap,

    pub(crate) storage_logs: ReversableLog<Box<StorageLogQuery>>,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct PrecompileProcessorTestInnerState {
    pub(crate) timestamp_history: HistoryRecorder<Vec<Timestamp>>,
}

/// A struct that encapsulates the state of the VM's oracles
/// The state is to be used in tests.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct VmInstanceInnerState {
    event_sink: InMemoryEventSink,
    precompile_processor_state: PrecompileProcessorTestInnerState,
    memory: SimpleMemory,
    decommitter_state: DecommitterTestInnerState,
    storage_oracle_state: StorageOracleInnerState,
    local_state: VmLocalState,
}

impl<S: WriteStorage> Vm<S> {
    // Dump inner state of the VM.
    pub(crate) fn dump_inner_state(&self) -> VmInstanceInnerState {
        let event_sink = self.state.event_sink.clone();
        let precompile_processor_state = PrecompileProcessorTestInnerState {
            timestamp_history: self.state.precompiles_processor.timestamp_history.clone(),
        };
        let memory = self.state.memory.clone();
        let decommitter_state = DecommitterTestInnerState {
            modified_storage_keys: ModifiedKeysMap(
                self.state
                    .decommittment_processor
                    .get_storage()
                    .borrow()
                    .modified_storage_keys()
                    .clone(),
            ),
            known_bytecodes: self.state.decommittment_processor.known_bytecodes.clone(),
            decommitted_code_hashes: self
                .state
                .decommittment_processor
                .get_decommitted_code_hashes_with_history()
                .clone(),
        };
        let storage_oracle_state = StorageOracleInnerState {
            modified_storage_keys: ModifiedKeysMap(
                self.state
                    .storage
                    .get_ptr()
                    .borrow()
                    .modified_storage_keys()
                    .clone(),
            ),
            storage_logs: self.state.storage.storage_logs(),
        };
        let local_state = self.state.local_state.clone();

        VmInstanceInnerState {
            event_sink,
            precompile_processor_state,
            memory,
            decommitter_state,
            storage_oracle_state,
            local_state,
        }
    }
}
