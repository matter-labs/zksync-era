use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use once_cell::sync::OnceCell;
use zk_evm_1_5_2::{
    aux_structures::{MemoryPage, Timestamp},
    vm_state::VmLocalState,
    zkevm_opcode_defs::{ContractCodeSha256Format, VersionedHashLen32},
};
use zksync_types::{
    bytecode::BytecodeHash, l2::L2Tx, vm::VmVersion, writes::StateDiffRecord, StorageKey,
    StorageValue, Transaction, H256, U256,
};
use zksync_vm_interface::{Call, InspectExecutionMode, VmInterface};

use super::{HistoryEnabled, ToTracerPointer, Vm};
use crate::{
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        storage::{InMemoryStorage, ReadStorage, StorageView, WriteStorage},
        tracer::ViolatedValidationRule,
        CurrentExecutionState, L2BlockEnv, VmExecutionMode, VmExecutionResultAndLogs,
    },
    tracers::{CallTracer, StorageInvocations, ValidationTracer},
    utils::bytecode::bytes_to_be_words,
    versions::testonly::{
        filter_out_base_system_contracts, validation_params, TestedVm, TestedVmForValidation,
        TestedVmWithCallTracer, TestedVmWithStorageLimit,
    },
    vm_latest::{
        constants::BOOTLOADER_HEAP_PAGE,
        old_vm::{event_sink::InMemoryEventSink, history_recorder::HistoryRecorder},
        tracers::PubdataTracer,
        types::TransactionData,
        utils::logs::StorageLogQuery,
        AppDataFrameManagerWithHistory, HistoryMode, SimpleMemory, TracerDispatcher,
    },
};

mod bootloader;
mod default_aa;
// TODO - fix this test
// `mod invalid_bytecode;`
mod account_validation_rules;
mod block_tip;
mod bytecode_publishing;
mod call_tracer;
mod circuits;
mod code_oracle;
mod constants;
mod evm;
mod gas_limit;
mod get_used_contracts;
mod is_write_initial;
mod l1_messenger;
mod l1_tx_execution;
mod l2_blocks;
mod mock_evm;
mod nonce_holder;
mod precompiles;
mod prestate_tracer;
mod refunds;
mod require_eip712;
mod rollbacks;
mod secp256r1;
mod simple_execution;
mod storage;
mod tracing_execution_error;
mod transfer;
mod upgrade;

type TestedLatestVm = Vm<StorageView<InMemoryStorage>, HistoryEnabled>;

impl TestedVm for TestedLatestVm {
    type StateDump = VmInstanceInnerState<HistoryEnabled>;

    fn dump_state(&self) -> Self::StateDump {
        self.dump_inner_state()
    }

    fn gas_remaining(&mut self) -> u32 {
        self.state.local_state.callstack.current.ergs_remaining
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        self.get_current_execution_state()
    }

    fn decommitted_hashes(&self) -> HashSet<U256> {
        self.get_used_contracts().into_iter().collect()
    }

    fn finish_batch_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        pubdata_builder: Rc<dyn PubdataBuilder>,
    ) -> VmExecutionResultAndLogs {
        let pubdata_tracer = PubdataTracer::new_with_forced_state_diffs(
            self.batch_env.clone(),
            VmExecutionMode::Batch,
            diffs,
            crate::vm_latest::MultiVmSubversion::latest(),
            Some(pubdata_builder),
        );
        self.inspect_inner(
            &mut TracerDispatcher::default(),
            VmExecutionMode::Batch,
            Some(pubdata_tracer),
        )
    }

    fn finish_batch_without_pubdata(&mut self) -> VmExecutionResultAndLogs {
        self.inspect_inner(
            &mut TracerDispatcher::default(),
            VmExecutionMode::Batch,
            None,
        )
    }

    fn insert_bytecodes(&mut self, bytecodes: &[&[u8]]) {
        let bytecodes = bytecodes
            .iter()
            .map(|&bytecode| {
                let hash = BytecodeHash::for_bytecode(bytecode).value_u256();
                let words = bytes_to_be_words(bytecode);
                (hash, words)
            })
            .collect();
        self.state
            .decommittment_processor
            .populate(bytecodes, Timestamp(0));
    }

    fn known_bytecode_hashes(&self) -> HashSet<U256> {
        let mut bytecode_hashes: HashSet<_> = self
            .state
            .decommittment_processor
            .known_bytecodes
            .inner()
            .keys()
            .copied()
            .collect();
        filter_out_base_system_contracts(&mut bytecode_hashes);
        bytecode_hashes
    }

    fn manually_decommit(&mut self, code_hash: H256) -> bool {
        let (header, normalized_preimage) =
            ContractCodeSha256Format::normalize_for_decommitment(&code_hash.0);
        let query = self
            .state
            .prepare_to_decommit(
                0,
                header,
                normalized_preimage,
                MemoryPage(123),
                Timestamp(0),
            )
            .unwrap();
        self.state.execute_decommit(0, query).unwrap();
        query.is_fresh
    }

    fn verify_required_bootloader_heap(&self, cells: &[(u32, U256)]) {
        for &(slot, required_value) in cells {
            let current_value = self
                .state
                .memory
                .read_slot(BOOTLOADER_HEAP_PAGE as usize, slot as usize)
                .value;
            assert_eq!(current_value, required_value);
        }
    }

    fn write_to_bootloader_heap(&mut self, cells: &[(usize, U256)]) {
        let timestamp = Timestamp(self.state.local_state.timestamp);
        self.state
            .memory
            .populate_page(BOOTLOADER_HEAP_PAGE as usize, cells.to_vec(), timestamp)
    }

    fn read_storage(&mut self, key: StorageKey) -> U256 {
        self.state.storage.storage.read_from_storage(&key)
    }

    fn last_l2_block_hash(&self) -> H256 {
        self.bootloader_state.last_l2_block().get_hash()
    }

    fn push_l2_block_unchecked(&mut self, block: L2BlockEnv) {
        self.bootloader_state.push_l2_block(block);
    }

    fn push_transaction_with_refund(&mut self, tx: Transaction, refund: u64) {
        let tx = TransactionData::new(tx, false);
        let overhead = tx.overhead_gas();
        self.push_raw_transaction(tx, overhead, refund, true)
    }

    fn pubdata_input(&self) -> PubdataInput {
        self.bootloader_state.get_pubdata_information().clone()
    }
}

impl TestedVmForValidation for TestedLatestVm {
    fn run_validation(
        &mut self,
        tx: L2Tx,
        timestamp: u64,
    ) -> (VmExecutionResultAndLogs, Option<ViolatedValidationRule>) {
        let validation_params = validation_params(&tx, &self.system_env);
        self.push_transaction(tx.into());

        let tracer = ValidationTracer::<HistoryEnabled>::new(
            validation_params,
            VmVersion::VmEcPrecompiles,
            timestamp,
        );
        let mut failures = tracer.get_result();

        let result = self.inspect_inner(
            &mut tracer.into_tracer_pointer().into(),
            VmExecutionMode::OneTx,
            None,
        );
        let violated_rule = Arc::make_mut(&mut failures).take();
        (result, violated_rule)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ModifiedKeysMap(HashMap<StorageKey, StorageValue>);

impl ModifiedKeysMap {
    fn new<S: ReadStorage>(storage: &mut StorageView<S>) -> Self {
        let mut modified_keys = storage.modified_storage_keys().clone();
        let inner = storage.inner_mut();
        // Remove modified keys that were set to the same value (e.g., due to a rollback).
        modified_keys.retain(|key, value| inner.read_value(key) != *value);
        Self(modified_keys)
    }
}

// We consider hashmaps to be equal even if there is a key
// that is not present in one but has zero value in another.
impl PartialEq for ModifiedKeysMap {
    fn eq(&self, other: &Self) -> bool {
        for (key, value) in &self.0 {
            if *value != other.0.get(key).copied().unwrap_or_default() {
                return false;
            }
        }
        for (key, value) in &other.0 {
            if *value != self.0.get(key).copied().unwrap_or_default() {
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
    pub(crate) decommitted_code_hashes: HistoryRecorder<HashMap<U256, Option<u32>>, HistoryEnabled>,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct StorageOracleInnerState<H: HistoryMode> {
    /// There is no way to "truly" compare the storage pointer,
    /// so we just compare the modified keys. This is reasonable enough.
    pub(crate) modified_storage_keys: ModifiedKeysMap,
    pub(crate) frames_stack: AppDataFrameManagerWithHistory<Box<StorageLogQuery>, H>,
    pub(crate) paid_changes: HistoryRecorder<HashMap<StorageKey, u32>, H>,
    pub(crate) initial_values: HistoryRecorder<HashMap<StorageKey, U256>, H>,
    pub(crate) returned_io_refunds: HistoryRecorder<Vec<u32>, H>,
    pub(crate) returned_pubdata_costs: HistoryRecorder<Vec<i32>, H>,
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

impl<S: ReadStorage, H: crate::glue::history_mode::HistoryMode> Vm<StorageView<S>, H> {
    // Dump inner state of the VM.
    pub(crate) fn dump_inner_state(&self) -> VmInstanceInnerState<H::Vm1_5_2> {
        let event_sink = self.state.event_sink.clone();
        let precompile_processor_state = PrecompileProcessorTestInnerState {
            timestamp_history: self.state.precompiles_processor.timestamp_history.clone(),
        };
        let memory = self.state.memory.clone();
        let decommitter_state = DecommitterTestInnerState {
            modified_storage_keys: ModifiedKeysMap::new(
                &mut self
                    .state
                    .decommittment_processor
                    .get_storage()
                    .borrow_mut(),
            ),
            known_bytecodes: self.state.decommittment_processor.known_bytecodes.clone(),
            decommitted_code_hashes: self
                .state
                .decommittment_processor
                .get_decommitted_code_hashes_with_history()
                .clone(),
        };

        let storage_oracle_state = StorageOracleInnerState {
            modified_storage_keys: ModifiedKeysMap::new(
                &mut self.state.storage.storage.get_ptr().borrow_mut(),
            ),
            frames_stack: self.state.storage.storage_frames_stack.clone(),
            paid_changes: self.state.storage.paid_changes.clone(),
            initial_values: self.state.storage.initial_values.clone(),
            returned_io_refunds: self.state.storage.returned_io_refunds.clone(),
            returned_pubdata_costs: self.state.storage.returned_pubdata_costs.clone(),
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

impl TestedVmWithCallTracer for TestedLatestVm {
    fn inspect_with_call_tracer(&mut self) -> (VmExecutionResultAndLogs, Vec<Call>) {
        let result = Arc::new(OnceCell::new());
        let call_tracer = CallTracer::new(result.clone()).into_tracer_pointer();
        let res = self.inspect(&mut call_tracer.into(), InspectExecutionMode::OneTx);
        let traces = result.get().unwrap().clone();
        (res, traces)
    }
}

impl TestedVmWithStorageLimit for TestedLatestVm {
    fn execute_with_storage_limit(&mut self, limit: usize) -> VmExecutionResultAndLogs {
        let tracer = StorageInvocations::new(limit).into_tracer_pointer();
        self.inspect(&mut tracer.into(), InspectExecutionMode::OneTx)
    }
}
