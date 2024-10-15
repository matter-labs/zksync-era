use std::collections::HashMap;

use zk_evm_1_5_0::{aux_structures::Timestamp, vm_state::VmLocalState};
use zksync_types::{StorageKey, StorageValue, U256};
use zksync_vm_interface::storage::StorageView;

use crate::{
    interface::storage::{ReadStorage, WriteStorage},
    vm_latest::{
        old_vm::{
            event_sink::InMemoryEventSink,
            history_recorder::{AppDataFrameManagerWithHistory, HistoryRecorder},
        },
        utils::logs::StorageLogQuery,
        HistoryEnabled, HistoryMode, SimpleMemory, Vm,
    },
    HistoryMode as CommonHistoryMode,
};
