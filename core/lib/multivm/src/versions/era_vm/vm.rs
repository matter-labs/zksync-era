use era_vm::VMState;
use zksync_state::{ReadStorage, StoragePtr};
use zksync_types::Transaction;
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::{
    interface::{VmInterface, VmInterfaceHistoryEnabled},
    vm_latest,
    vm_latest::{
        BootloaderMemory, CurrentExecutionState, ExecutionResult, HistoryEnabled, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmExecutionLogs, VmExecutionMode, VmExecutionResultAndLogs,
    },
};

pub struct Vm<S: ReadStorage> {
    pub(crate) inner: VMState,
    suspended_at: u16,
    gas_for_account_validation: u32,
    last_tx_result: Option<ExecutionResult>,

    bootloader_state: BootloaderState, // TODO: Implement bootloader state logic
    pub(crate) storage: StoragePtr<S>,

    // TODO: Maybe not necessary, check
    // program_cache: Rc<RefCell<HashMap<U256, Program>>>,

    // these two are only needed for tests so far
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,

    snapshots: Vec<VmSnapshot>, // TODO: Implement snapshots logic
}

impl<S: ReadStorage + 'static> VmInterface<S, HistoryEnabled> for Vm<S> {
    type TracerDispatcher = ();

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        todo!()
    }

    fn push_transaction(&mut self, tx: Transaction) {
        todo!()
    }

    fn inspect(
        &mut self,
        _tracer: Self::TracerDispatcher,
        _execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        todo!()
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        todo!()
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        todo!()
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        todo!()
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        todo!()
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (
        Result<(), crate::interface::BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        todo!()
    }

    fn record_vm_memory_metrics(&self) -> crate::vm_1_4_1::VmMemoryMetrics {
        todo!()
    }

    fn gas_remaining(&self) -> u32 {
        todo!()
    }
}

impl<S: ReadStorage + 'static> VmInterfaceHistoryEnabled<S> for Vm<S> {
    fn make_snapshot(&mut self) {
        todo!()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        todo!()
    }

    fn pop_snapshot_no_rollback(&mut self) {
        todo!()
    }
}
