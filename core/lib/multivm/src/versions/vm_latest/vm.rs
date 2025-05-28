use std::{collections::HashMap, rc::Rc};

use circuit_sequencer_api::sort_storage_access::sort_storage_access_queries;
use zksync_types::{
    h256_to_u256,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    u256_to_h256,
    vm::VmVersion,
    Transaction, H256,
};
use zksync_vm_interface::{pubdata::PubdataBuilder, InspectExecutionMode};

use crate::{
    glue::GlueInto,
    interface::{
        storage::{StoragePtr, WriteStorage},
        BytecodeCompressionError, BytecodeCompressionResult, CurrentExecutionState,
        FinishedL1Batch, L1BatchEnv, L2BlockEnv, PushTransactionResult, SystemEnv, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
        VmTrackingContracts,
    },
    utils::{bytecode::be_words_to_bytes, events::extract_l2tol1logs_from_l1_messenger},
    vm_latest::{
        bootloader::BootloaderState,
        old_vm::{events::merge_events, history_recorder::HistoryEnabled},
        tracers::{dispatcher::TracerDispatcher, PubdataTracer},
        types::{new_vm_state, VmSnapshot, ZkSyncVmState},
    },
    HistoryMode,
};

/// MultiVM-specific addition.
///
/// In the first version of the v1.5.0 release, the bootloader memory was too small, so a new
/// version was released with increased bootloader memory. The version with the small bootloader memory
/// is available only on internal staging environments.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd)]
pub(crate) enum MultiVmSubversion {
    /// The initial version of v1.5.0, available only on staging environments.
    SmallBootloaderMemory,
    /// The final correct version of v1.5.0
    IncreasedBootloaderMemory,
    /// VM for post-gateway versions.
    Gateway,
    EvmEmulator,
    EcPrecompiles,
    Interop,
}

impl MultiVmSubversion {
    pub(crate) fn latest() -> Self {
        Self::Interop
    }
}

#[derive(Debug)]
pub(crate) struct VmVersionIsNotVm150Error;

impl TryFrom<VmVersion> for MultiVmSubversion {
    type Error = VmVersionIsNotVm150Error;

    fn try_from(value: VmVersion) -> Result<Self, Self::Error> {
        match value {
            VmVersion::Vm1_5_0SmallBootloaderMemory => Ok(Self::SmallBootloaderMemory),
            VmVersion::Vm1_5_0IncreasedBootloaderMemory => Ok(Self::IncreasedBootloaderMemory),
            VmVersion::VmGateway => Ok(Self::Gateway),
            VmVersion::VmEvmEmulator => Ok(Self::EvmEmulator),
            VmVersion::VmEcPrecompiles => Ok(Self::EcPrecompiles),
            VmVersion::VmInterop => Ok(Self::Interop),
            _ => Err(VmVersionIsNotVm150Error),
        }
    }
}

/// Main entry point for Virtual Machine integration.
/// The instance should process only one l1 batch
#[derive(Debug)]
pub struct Vm<S: WriteStorage, H: HistoryMode> {
    pub(crate) bootloader_state: BootloaderState,
    // Current state and oracles of virtual machine
    pub(crate) state: ZkSyncVmState<S, H::Vm1_5_2>,
    pub(crate) storage: StoragePtr<S>,
    pub(crate) system_env: SystemEnv,
    pub(crate) batch_env: L1BatchEnv,
    // Snapshots for the current run
    pub(crate) snapshots: Vec<VmSnapshot>,
    pub(crate) subversion: MultiVmSubversion,
    _phantom: std::marker::PhantomData<H>,
}

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(super) fn gas_remaining(&self) -> u32 {
        self.state.local_state.callstack.current.ergs_remaining
    }

    pub(crate) fn decommit_dynamic_bytecodes(
        &self,
        candidate_hashes: impl Iterator<Item = H256>,
    ) -> HashMap<H256, Vec<u8>> {
        let decommitter = &self.state.decommittment_processor;
        let bytecodes = candidate_hashes.filter_map(|hash| {
            let int_hash = h256_to_u256(hash);
            if !decommitter.dynamic_bytecode_hashes.contains(&int_hash) {
                return None;
            }
            let bytecode = decommitter
                .known_bytecodes
                .inner()
                .get(&int_hash)
                .unwrap_or_else(|| {
                    panic!("Bytecode with hash {hash:?} not found");
                });
            Some((hash, be_words_to_bytes(bytecode)))
        });
        bytecodes.collect()
    }

    // visible for testing
    pub(super) fn get_current_execution_state(&self) -> CurrentExecutionState {
        let (raw_events, l1_messages) = self.state.event_sink.flatten();
        let events: Vec<_> = merge_events(raw_events)
            .into_iter()
            .map(|e| e.into_vm_event(self.batch_env.number))
            .collect();

        let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events);
        let system_logs = l1_messages
            .into_iter()
            .map(|log| SystemL2ToL1Log(log.glue_into()))
            .collect();

        let storage_log_queries = self.state.storage.get_final_log_queries();
        let deduped_storage_log_queries =
            sort_storage_access_queries(storage_log_queries.iter().map(|log| log.log_query)).1;

        CurrentExecutionState {
            events,
            deduplicated_storage_logs: deduped_storage_log_queries
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            used_contract_hashes: self.get_used_contracts(),
            user_l2_to_l1_logs: user_l2_to_l1_logs
                .into_iter()
                .map(|log| UserL2ToL1Log(log.into()))
                .collect(),
            system_logs,
            storage_refunds: self.state.storage.returned_io_refunds.inner().clone(),
            pubdata_costs: self.state.storage.returned_pubdata_costs.inner().clone(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmInterface for Vm<S, H> {
    type TracerDispatcher = TracerDispatcher<S, H::Vm1_5_2>;

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_> {
        self.push_transaction_with_compression(tx, true);
        PushTransactionResult {
            compressed_bytecodes: self
                .bootloader_state
                .get_last_tx_compressed_bytecodes()
                .into(),
        }
    }

    /// Execute VM with custom tracers.
    fn inspect(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inspect_inner(tracer, execution_mode.into(), None)
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env);
    }

    /// Inspect transaction with optional bytecode compression.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        self.push_transaction_with_compression(tx, with_compression);
        let result = self.inspect_inner(tracer, VmExecutionMode::OneTx, None);
        if self.has_unpublished_bytecodes() {
            (
                Err(BytecodeCompressionError::BytecodeCompressionFailed),
                result,
            )
        } else {
            (
                Ok(self
                    .bootloader_state
                    .get_last_tx_compressed_bytecodes()
                    .into()),
                result,
            )
        }
    }

    fn finish_batch(&mut self, pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        let pubdata_tracer = Some(PubdataTracer::new(
            self.batch_env.clone(),
            VmExecutionMode::Batch,
            self.subversion,
            Some(pubdata_builder.clone()),
        ));

        let result = self.inspect_inner(
            &mut TracerDispatcher::default(),
            VmExecutionMode::Batch,
            pubdata_tracer,
        );
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self
            .bootloader_state
            .bootloader_memory(pubdata_builder.as_ref());
        FinishedL1Batch {
            block_tip_execution_result: result,
            final_execution_state: execution_state,
            final_bootloader_memory: Some(bootloader_memory),
            pubdata_input: Some(
                self.bootloader_state
                    .settlement_layer_pubdata(pubdata_builder.as_ref()),
            ),
            state_diffs: Some(
                self.bootloader_state
                    .get_pubdata_information()
                    .state_diffs
                    .clone(),
            ),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmFactory<S> for Vm<S, H> {
    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let vm_version: VmVersion = system_env.version.into();
        Self::new_with_subversion(
            batch_env,
            system_env,
            storage,
            vm_version.try_into().expect("Incorrect 1.5.0 VmVersion"),
        )
    }
}

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn new_with_subversion(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<S>,
        subversion: MultiVmSubversion,
    ) -> Self {
        let (state, bootloader_state) =
            new_vm_state(storage.clone(), &system_env, &batch_env, subversion);
        Self {
            bootloader_state,
            state,
            storage,
            system_env,
            batch_env,
            subversion,
            snapshots: vec![],
            _phantom: Default::default(),
        }
    }
}

impl<S: WriteStorage> VmInterfaceHistoryEnabled for Vm<S, HistoryEnabled> {
    fn make_snapshot(&mut self) {
        self.make_snapshot_inner()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let snapshot = self
            .snapshots
            .pop()
            .expect("Snapshot should be created before rolling it back");
        self.rollback_to_snapshot(snapshot);
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots.pop();
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTrackingContracts for Vm<S, H> {
    fn used_contract_hashes(&self) -> Vec<H256> {
        self.get_used_contracts()
            .into_iter()
            .map(u256_to_h256)
            .collect()
    }
}
