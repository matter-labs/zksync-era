use std::marker::PhantomData;
use zk_evm_1_4_0::{
    aux_structures::Timestamp,
    tracing::{BeforeExecutionData, VmLocalStateData},
};

use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    event::{
        extract_bytecode_publication_requests_from_l1_messenger,
        extract_l2tol1logs_from_l1_messenger, extract_long_l2_to_l1_messages, L1MessengerL2ToL1Log,
    },
    writes::StateDiffRecord,
    zkevm_test_harness::witness::sort_storage_access::sort_storage_access_queries,
    AccountTreeId, StorageKey, L1_MESSENGER_ADDRESS,
};
use zksync_utils::u256_to_h256;
use zksync_utils::{h256_to_u256, u256_to_bytes_be};

use crate::vm_latest::{
    old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
    types::internals::pubdata::PubdataInput,
};
use crate::{vm_latest::constants::BOOTLOADER_HEAP_PAGE, vm_latest::StorageOracle};

use crate::interface::dyn_tracers::vm_1_4_0::DynTracer;
use crate::interface::tracer::{TracerExecutionStatus, TracerExecutionStopReason};
use crate::interface::types::inputs::L1BatchEnv;
use crate::vm_latest::tracers::{traits::VmTracer, utils::VmHook};
use crate::vm_latest::types::internals::ZkSyncVmState;
use crate::vm_latest::utils::logs::collect_events_and_l1_system_logs_after_timestamp;
use crate::{
    interface::VmExecutionMode,
    vm_latest::bootloader_state::{utils::apply_pubdata_to_memory, BootloaderState},
};

/// Tracer responsible for collecting information about refunds.
#[derive(Debug, Clone)]
pub(crate) struct PubdataTracer<S> {
    l1_batch_env: L1BatchEnv,
    pubdata_info_requested: bool,
    execution_mode: VmExecutionMode,
    _phantom_data: PhantomData<S>,
}

impl<S: WriteStorage> PubdataTracer<S> {
    pub(crate) fn new(l1_batch_env: L1BatchEnv, execution_mode: VmExecutionMode) -> Self {
        Self {
            l1_batch_env,
            pubdata_info_requested: false,
            execution_mode,
            _phantom_data: Default::default(),
        }
    }
}

impl<S: WriteStorage> PubdataTracer<S> {
    // Packs part of L1 Messenger total pubdata that corresponds to
    // L2toL1Logs sent in the block
    fn get_total_user_logs<H: HistoryMode>(
        &self,
        state: &ZkSyncVmState<S, H>,
    ) -> Vec<L1MessengerL2ToL1Log> {
        let (all_generated_events, _) = collect_events_and_l1_system_logs_after_timestamp(
            state,
            &self.l1_batch_env,
            Timestamp(0),
        );
        extract_l2tol1logs_from_l1_messenger(&all_generated_events)
    }

    // Packs part of L1 Messenger total pubdata that corresponds to
    // Messages sent in the block
    fn get_total_l1_messenger_messages<H: HistoryMode>(
        &self,
        state: &ZkSyncVmState<S, H>,
    ) -> Vec<Vec<u8>> {
        let (all_generated_events, _) = collect_events_and_l1_system_logs_after_timestamp(
            state,
            &self.l1_batch_env,
            Timestamp(0),
        );

        extract_long_l2_to_l1_messages(&all_generated_events)
    }

    // Packs part of L1 Messenger total pubdata that corresponds to
    // Bytecodes needed to be published on L1
    fn get_total_published_bytecodes<H: HistoryMode>(
        &self,
        state: &ZkSyncVmState<S, H>,
    ) -> Vec<Vec<u8>> {
        let (all_generated_events, _) = collect_events_and_l1_system_logs_after_timestamp(
            state,
            &self.l1_batch_env,
            Timestamp(0),
        );

        let bytecode_publication_requests =
            extract_bytecode_publication_requests_from_l1_messenger(&all_generated_events);

        bytecode_publication_requests
            .iter()
            .map(|bytecode_publication_request| {
                state
                    .decommittment_processor
                    .known_bytecodes
                    .inner()
                    .get(&h256_to_u256(bytecode_publication_request.bytecode_hash))
                    .unwrap()
                    .iter()
                    .flat_map(u256_to_bytes_be)
                    .collect()
            })
            .collect()
    }

    // Packs part of L1Messenger total pubdata that corresponds to
    // State diffs needed to be published on L1
    fn get_state_diffs<H: HistoryMode>(storage: &StorageOracle<S, H>) -> Vec<StateDiffRecord> {
        sort_storage_access_queries(
            storage
                .storage_log_queries_after_timestamp(Timestamp(0))
                .iter()
                .map(|log| &log.log_query),
        )
        .1
        .into_iter()
        .filter(|log| log.rw_flag)
        .filter(|log| log.read_value != log.written_value)
        .filter(|log| log.address != L1_MESSENGER_ADDRESS)
        .map(|log| StateDiffRecord {
            address: log.address,
            key: log.key,
            derived_key: log.derive_final_address(),
            enumeration_index: storage
                .storage
                .get_ptr()
                .borrow_mut()
                .get_enumeration_index(&StorageKey::new(
                    AccountTreeId::new(log.address),
                    u256_to_h256(log.key),
                ))
                .unwrap_or_default(),
            initial_value: log.read_value,
            final_value: log.written_value,
        })
        .collect()
    }

    fn build_pubdata_input<H: HistoryMode>(&self, state: &ZkSyncVmState<S, H>) -> PubdataInput {
        PubdataInput {
            user_logs: self.get_total_user_logs(state),
            l2_to_l1_messages: self.get_total_l1_messenger_messages(state),
            published_bytecodes: self.get_total_published_bytecodes(state),
            state_diffs: Self::get_state_diffs(&state.storage),
        }
    }
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for PubdataTracer<S> {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        let hook = VmHook::from_opcode_memory(&state, &data);
        if let VmHook::PubdataRequested = hook {
            self.pubdata_info_requested = true;
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for PubdataTracer<S> {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        if !matches!(self.execution_mode, VmExecutionMode::Batch) {
            // We do not provide the pubdata when executing the block tip or a single transaction
            if self.pubdata_info_requested {
                return TracerExecutionStatus::Stop(TracerExecutionStopReason::Finish);
            } else {
                return TracerExecutionStatus::Continue;
            }
        }

        if self.pubdata_info_requested {
            let pubdata_input = self.build_pubdata_input(state);

            // Save the pubdata for the future initial bootloader memory building
            bootloader_state.set_pubdata_input(pubdata_input.clone());

            // Apply the pubdata to the current memory
            let mut memory_to_apply = vec![];
            apply_pubdata_to_memory(&mut memory_to_apply, pubdata_input);
            state.memory.populate_page(
                BOOTLOADER_HEAP_PAGE as usize,
                memory_to_apply,
                Timestamp(state.local_state.timestamp),
            );

            self.pubdata_info_requested = false;
        }

        TracerExecutionStatus::Continue
    }
}
