use std::rc::Rc;

use zksync_types::{h256_to_u256, vm::VmVersion, Transaction};
use zksync_vm_interface::{pubdata::PubdataBuilder, InspectExecutionMode};

use crate::{
    glue::{history_mode::HistoryMode, GlueInto},
    interface::{
        storage::StoragePtr, BytecodeCompressionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
        PushTransactionResult, SystemEnv, TxExecutionMode, VmExecutionResultAndLogs, VmFactory,
        VmInterface, VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    vm_m5::{
        storage::Storage,
        vm_instance::{MultiVmSubversion, VmInstance},
    },
};

#[derive(Debug)]
pub struct Vm<S: Storage, H: HistoryMode> {
    pub(crate) vm: VmInstance<S>,
    pub(crate) system_env: SystemEnv,
    _phantom: std::marker::PhantomData<H>,
}

impl<S: Storage, H: HistoryMode> Vm<S, H> {
    pub fn new_with_subversion(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<S>,
        vm_sub_version: MultiVmSubversion,
    ) -> Self {
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
            system_env.bootloader_gas_limit,
        );
        Self {
            vm: inner_vm,
            system_env,
            _phantom: Default::default(),
        }
    }

    pub(crate) fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        VmMemoryMetrics::default()
    }
}

impl<S: Storage, H: HistoryMode> VmInterface for Vm<S, H> {
    /// Tracers are not supported for here we use `()` as a placeholder
    type TracerDispatcher = ();

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_> {
        crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
        );
        PushTransactionResult {
            compressed_bytecodes: (&[]).into(), // bytecode compression isn't supported
        }
    }

    fn inspect(
        &mut self,
        _tracer: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        match execution_mode {
            InspectExecutionMode::OneTx => match self.system_env.execution_mode {
                TxExecutionMode::VerifyExecute => self.vm.execute_next_tx().glue_into(),
                TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self
                    .vm
                    .execute_till_block_end(
                        crate::vm_m5::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                    )
                    .glue_into(),
            },
            InspectExecutionMode::Bootloader => self.vm.execute_block_tip().glue_into(),
        }
    }

    fn start_new_l2_block(&mut self, _l2_block_env: L2BlockEnv) {
        // Do nothing, because vm 1.3.2 doesn't support L2 blocks
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        _tracer: &mut Self::TracerDispatcher,
        tx: Transaction,
        _with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
        );
        // Bytecode compression isn't supported
        (
            Ok(vec![].into()),
            self.inspect(&mut (), InspectExecutionMode::OneTx),
        )
    }

    fn finish_batch(&mut self, _pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        self.vm
            .execute_till_block_end(
                crate::vm_m5::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
            )
            .glue_into()
    }

    fn gas_remaining(&mut self) -> u32 {
        self.vm.gas_remaining()
    }
}

impl<S: Storage, H: HistoryMode> VmFactory<S> for Vm<S, H> {
    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let vm_version: VmVersion = system_env.version.into();
        let vm_sub_version = match vm_version {
            VmVersion::M5WithoutRefunds => MultiVmSubversion::V1,
            VmVersion::M5WithRefunds => MultiVmSubversion::V2,
            _ => panic!("Unsupported protocol version for vm_m5: {:?}", vm_version),
        };
        Self::new_with_subversion(batch_env, system_env, storage, vm_sub_version)
    }
}

impl<S: Storage> VmInterfaceHistoryEnabled for Vm<S, crate::vm_latest::HistoryEnabled> {
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
