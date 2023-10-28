use crate::interface::{
    BootloaderMemory, BytecodeCompressionError, CurrentExecutionState, FinishedL1Batch, L1BatchEnv,
    L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs, VmInterface,
    VmInterfaceHistoryEnabled, VmMemoryMetrics,
};
use std::any::Any;

use zksync_state::StoragePtr;
use zksync_types::{Transaction, VmVersion};
use zksync_utils::bytecode::CompressedBytecodeInfo;
use zksync_utils::h256_to_u256;

use crate::glue::history_mode::HistoryMode;
use crate::glue::GlueInto;
use crate::vm_m5::storage::Storage;
use crate::vm_m5::vm_instance::MultiVMSubversion;
use crate::vm_m5::vm_instance::VmInstance;

#[derive(Debug)]
pub struct Vm<S: Storage, H: HistoryMode> {
    pub(crate) vm: VmInstance<S>,
    pub(crate) system_env: SystemEnv,
    pub(crate) last_tx_compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    _phantom: std::marker::PhantomData<H>,
}

impl<S: Storage, H: HistoryMode> VmInterface<S, H> for Vm<S, H> {
    /// Tracers are not supported for vm 1.3.2. So we use `Vec<Box<dyn Any>>` as a placeholder
    type TracerDispatcher = Vec<Box<dyn Any>>;

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let vm_version: VmVersion = system_env.version.into();
        let vm_sub_version = match vm_version {
            VmVersion::M5WithoutRefunds => MultiVMSubversion::V1,
            VmVersion::M5WithRefunds => MultiVMSubversion::V2,
            _ => panic!("Unsupported protocol version for vm_m6: {:?}", vm_version),
        };
        let oracle_tools = crate::vm_m5::OracleTools::new(storage.clone(), vm_sub_version);
        let block_properties = zk_evm_1_3_1::block_properties::BlockProperties {
            default_aa_code_hash: h256_to_u256(
                system_env.base_system_smart_contracts.default_aa.hash,
            ),
            zkporter_is_available: false,
        };
        let inner_vm = crate::vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
            vm_sub_version,
            oracle_tools,
            batch_env.glue_into(),
            block_properties,
            system_env.execution_mode.glue_into(),
            &system_env.base_system_smart_contracts.clone().glue_into(),
            system_env.gas_limit,
        );
        Self {
            vm: inner_vm,
            system_env,
            last_tx_compressed_bytecodes: vec![],
            _phantom: Default::default(),
        }
    }

    fn push_transaction(&mut self, tx: Transaction) {
        crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
        )
    }

    fn inspect(
        &mut self,
        _tracer: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        match execution_mode {
            VmExecutionMode::OneTx => {
                match self.system_env.execution_mode {
                    TxExecutionMode::VerifyExecute => {
                        // Even that call tracer is supported by vm vm1.3.2, we don't use it for multivm
                        self.vm.execute_next_tx()
                            .glue_into()
                    }
                    TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self.vm
                        .execute_till_block_end(
                            crate::vm_m5::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                        )
                        .glue_into(),
                }
            }
            VmExecutionMode::Batch => {
                panic!("Not supported for vm before vm with virtual blocks, use `finish_batch` instead")
            }
            VmExecutionMode::Bootloader => self.vm.execute_block_tip().glue_into(),
        }
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        vec![]
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        self.last_tx_compressed_bytecodes.clone()
    }

    fn start_new_l2_block(&mut self, _l2_block_env: L2BlockEnv) {
        // Do nothing, because vm 1.3.2 doesn't support L2 blocks
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        panic!("Not supported for vm before vm with virtual blocks, use `finish_batch` instead")
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        _tracer: Self::TracerDispatcher,
        tx: Transaction,
        _with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, BytecodeCompressionError> {
        crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
        );
        Ok(self.execute(VmExecutionMode::OneTx))
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        VmMemoryMetrics {
            event_sink_inner: 0,
            event_sink_history: 0,
            memory_inner: 0,
            memory_history: 0,
            decommittment_processor_inner: 0,
            decommittment_processor_history: 0,
            storage_inner: 0,
            storage_history: 0,
        }
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        self.vm
            .execute_till_block_end(
                crate::vm_m5::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
            )
            .glue_into()
    }
}

impl<S: Storage> VmInterfaceHistoryEnabled<S> for Vm<S, crate::vm_latest::HistoryEnabled> {
    fn make_snapshot(&mut self) {
        self.vm.save_current_vm_as_snapshot()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.vm.rollback_to_latest_snapshot_popping();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.vm.snapshots.pop();
    }
}
