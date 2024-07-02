use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};

use pretty_assertions::assert_eq;
use zksync_state::{ReadStorage, StoragePtr};
use zksync_types::{StorageKey, StorageLogWithPreviousValue, Transaction, U256};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::{
    interface::{
        BootloaderMemory, BytecodeCompressionError, CurrentExecutionState, FinishedL1Batch,
        L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode, VmExecutionResultAndLogs, VmInterface,
        VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    vm_latest::HistoryEnabled,
    HistoryMode,
};

#[derive(Debug)]
pub(crate) struct ShadowVm<T, U> {
    main: T,
    shadow: U,
}

// **NB.** Ordering matters for modifying operations; the old VM applies storage changes to the storage after committing transactions.
impl<S, T, U, H> VmInterface<S, H> for ShadowVm<T, U>
where
    S: ReadStorage,
    H: HistoryMode,
    T: VmInterface<S, H>,
    U: VmInterface<S, HistoryEnabled, TracerDispatcher = ()>,
{
    type TracerDispatcher = T::TracerDispatcher;

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        Self {
            main: T::new(batch_env.clone(), system_env.clone(), storage.clone()),
            shadow: U::new(batch_env, system_env, storage),
        }
    }

    fn push_transaction(&mut self, tx: Transaction) {
        self.shadow.push_transaction(tx.clone());
        self.main.push_transaction(tx);
    }

    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        let shadow_result = self.shadow.execute(execution_mode);
        let main_result = self.main.execute(execution_mode);
        assert_results_match(&main_result, &shadow_result);
        main_result
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.main.inspect(dispatcher, execution_mode)
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        let main_memory = self.main.get_bootloader_memory();
        let shadow_memory = self.shadow.get_bootloader_memory();
        assert_eq!(main_memory, shadow_memory);
        main_memory
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        let main_bytecodes = self.main.get_last_tx_compressed_bytecodes();
        let shadow_bytecodes = self.shadow.get_last_tx_compressed_bytecodes();
        assert_eq!(main_bytecodes, shadow_bytecodes);
        main_bytecodes
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.shadow.start_new_l2_block(l2_block_env);
        self.main.start_new_l2_block(l2_block_env);
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        let main_state = self.main.get_current_execution_state();
        let shadow_state = self.shadow.get_current_execution_state();
        assert_eq!(main_state, shadow_state);
        main_state
    }

    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        let shadow_result = self
            .shadow
            .execute_transaction_with_bytecode_compression(tx.clone(), with_compression);
        let main_result = self
            .main
            .execute_transaction_with_bytecode_compression(tx, with_compression);
        assert_results_match(&main_result.1, &shadow_result.1);
        main_result
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        let shadow_result = self.shadow.inspect_transaction_with_bytecode_compression(
            (),
            tx.clone(),
            with_compression,
        );
        let main_result =
            self.main
                .inspect_transaction_with_bytecode_compression(tracer, tx, with_compression);
        assert_results_match(&main_result.1, &shadow_result.1);
        main_result
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        self.main.record_vm_memory_metrics()
    }

    fn gas_remaining(&self) -> u32 {
        let main_gas = self.main.gas_remaining();
        let shadow_gas = self.shadow.gas_remaining();
        assert_eq!(main_gas, shadow_gas);
        main_gas
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let shadow_batch = self.shadow.finish_batch();
        let main_batch = self.main.finish_batch();
        assert_results_match(
            &main_batch.block_tip_execution_result,
            &shadow_batch.block_tip_execution_result,
        );
        assert_final_states_match(
            &main_batch.final_execution_state,
            &shadow_batch.final_execution_state,
        );
        assert_eq!(
            main_batch.final_bootloader_memory,
            shadow_batch.final_bootloader_memory
        );
        assert_eq!(main_batch.pubdata_input, shadow_batch.pubdata_input);
        assert_eq!(main_batch.state_diffs, shadow_batch.state_diffs);
        main_batch
    }
}

fn assert_results_match(
    main_result: &VmExecutionResultAndLogs,
    shadow_result: &VmExecutionResultAndLogs,
) {
    assert_eq!(main_result.result, shadow_result.result);
    assert_eq!(main_result.logs.events, shadow_result.logs.events);
    assert_eq!(
        main_result.logs.system_l2_to_l1_logs,
        shadow_result.logs.system_l2_to_l1_logs
    );
    assert_eq!(
        main_result.logs.user_l2_to_l1_logs,
        shadow_result.logs.user_l2_to_l1_logs
    );
    let main_logs = UniqueStorageLogs::new(&main_result.logs.storage_logs);
    let shadow_logs = UniqueStorageLogs::new(&shadow_result.logs.storage_logs);
    assert_eq!(
        main_logs, shadow_logs,
        "main: {main_logs:#?}\nshadow: {shadow_logs:#?}"
    );
    assert_eq!(main_result.refunds, shadow_result.refunds);
}

// The new VM doesn't support read logs yet, doesn't order logs by access and deduplicates them
// inside the VM, hence this auxiliary struct.
#[derive(PartialEq)]
struct UniqueStorageLogs(BTreeMap<StorageKey, StorageLogWithPreviousValue>);

impl fmt::Debug for UniqueStorageLogs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = formatter.debug_map();
        for log in self.0.values() {
            map.entry(
                &format!("{:?}:{:?}", log.log.key.address(), log.log.key.key()),
                &format!("{:?} -> {:?}", log.previous_value, log.log.value),
            );
        }
        map.finish()
    }
}

impl UniqueStorageLogs {
    fn new(logs: &[StorageLogWithPreviousValue]) -> Self {
        let mut unique_logs = BTreeMap::<StorageKey, StorageLogWithPreviousValue>::new();
        for log in logs {
            if !log.log.is_write() {
                continue;
            }
            if let Some(existing_log) = unique_logs.get_mut(&log.log.key) {
                existing_log.log.value = log.log.value;
            } else {
                unique_logs.insert(log.log.key, *log);
            }
        }
        Self(unique_logs)
    }
}

impl<S, T, U> VmInterfaceHistoryEnabled<S> for ShadowVm<T, U>
where
    S: ReadStorage,
    Self: VmInterface<S, HistoryEnabled>,
    T: VmInterfaceHistoryEnabled<S>,
    U: VmInterfaceHistoryEnabled<S>,
{
    fn make_snapshot(&mut self) {
        self.shadow.make_snapshot();
        self.main.make_snapshot();
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.shadow.rollback_to_the_latest_snapshot();
        self.main.rollback_to_the_latest_snapshot();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.shadow.pop_snapshot_no_rollback();
        self.main.pop_snapshot_no_rollback();
    }
}

fn assert_final_states_match(main: &CurrentExecutionState, shadow: &CurrentExecutionState) {
    assert_eq!(main.events, shadow.events);
    assert_eq!(main.user_l2_to_l1_logs, shadow.user_l2_to_l1_logs);
    assert_eq!(main.system_logs, shadow.system_logs);

    let main_deduplicated_logs: HashMap<_, _> = main
        .deduplicated_storage_logs
        .iter()
        .filter(|log| log.is_write())
        .map(|log| (log.key, log))
        .collect();
    let shadow_deduplicated_logs: HashMap<_, _> = shadow
        .deduplicated_storage_logs
        .iter()
        .filter(|log| log.is_write())
        .map(|log| (log.key, log))
        .collect();
    assert_eq!(main_deduplicated_logs, shadow_deduplicated_logs);
}
