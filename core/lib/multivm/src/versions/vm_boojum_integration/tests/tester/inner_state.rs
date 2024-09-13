use std::collections::HashMap;

use zk_evm_1_4_0::{aux_structures::Timestamp, vm_state::VmLocalState};
use crate::interface::storage::WriteStorage;
use zksync_types::{StorageKey, StorageLogQuery, StorageValue, U256};

use crate::{
    vm_boojum_integration::{
        old_vm::{
            event_sink::InMemoryEventSink,
            history_recorder::{AppDataFrameManagerWithHistory, HistoryRecorder},
        },
        HistoryEnabled, HistoryMode, SimpleMemory, Vm,
    },
    HistoryMode as CommonHistoryMode,
};

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
pub(crate) struct DecommitterTestInnerState<H: HistoryMode> {
    /// There is no way to "truly" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub(crate) modified_storage_keys: ModifiedKeysMap,
    pub(crate) known_bytecodes: HistoryRecorder<HashMap<U256, Vec<U256>>, H>,
    pub(crate) decommitted_code_hashes: HistoryRecorder<HashMap<U256, u32>, HistoryEnabled>,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct StorageOracleInnerState<H: HistoryMode> {
    /// There is no way to "truly" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub(crate) modified_storage_keys: ModifiedKeysMap,

    pub(crate) frames_stack: AppDataFrameManagerWithHistory<Box<StorageLogQuery>, H>,

    pub(crate) pre_paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, H>,
    pub(crate) paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, H>,
    pub(crate) initial_values: HistoryRecorder<HashMap<StorageKey, U256>, H>,
    pub(crate) returned_refunds: HistoryRecorder<Vec<u32>, H>,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct PrecompileProcessorTestInnerState<H: HistoryMode> {
    pub(crate) timestamp_history: HistoryRecorder<Vec<Timestamp>, H>,
}

/// A struct that encapsulates the state of the VM's oracles
/// The state is to be used in tests.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct VmInstanceInnerState<H: HistoryMode> {
    event_sink: InMemoryEventSink<H>,
    precompile_processor_state: PrecompileProcessorTestInnerState<H>,
    memory: SimpleMemory<H>,
    decommitter_state: DecommitterTestInnerState<H>,
    storage_oracle_state: StorageOracleInnerState<H>,
    local_state: VmLocalState,
}

impl<S: WriteStorage, H: CommonHistoryMode> Vm<S, H> {
    // Dump inner state of the VM.
    pub(crate) fn dump_inner_state(&self) -> VmInstanceInnerState<H::VmBoojumIntegration> {
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
                    .storage
                    .get_ptr()
                    .borrow()
                    .modified_storage_keys()
                    .clone(),
            ),
            frames_stack: self.state.storage.frames_stack.clone(),
            pre_paid_changes: self.state.storage.pre_paid_changes.clone(),
            paid_changes: self.state.storage.paid_changes.clone(),
            initial_values: self.state.storage.initial_values.clone(),
            returned_refunds: self.state.storage.returned_refunds.clone(),
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
