use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use zksync_types::{StorageKey, StorageLog, StorageLogWithPreviousValue, Transaction};

use crate::{
    dump::{VmDump, VmDumpHandler, VmStorageDump},
    interface::{
        storage::{ImmutableStorageView, ReadStorage, StoragePtr, StorageView},
        BootloaderMemory, BytecodeCompressionError, CompressedBytecodeInfo, CurrentExecutionState,
        FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
        VmMemoryMetrics,
    },
    vm_fast,
};

struct VmWithReporting<S, Shadow> {
    vm: Shadow,
    main_vm_storage: StoragePtr<StorageView<S>>,
    partial_dump: VmDump,
    dump_handler: VmDumpHandler,
    panic_on_divergence: bool,
}

impl<S: fmt::Debug, Shadow: fmt::Debug> fmt::Debug for VmWithReporting<S, Shadow> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VmWithReporting")
            .field("vm", &self.vm)
            .field("panic_on_divergence", &self.panic_on_divergence)
            .finish_non_exhaustive()
    }
}

impl<S: ReadStorage, Shadow: VmInterface> VmWithReporting<S, Shadow> {
    fn report(self, main_vm: &impl VmInterface, err: anyhow::Error) {
        let mut dump = self.partial_dump;
        let batch_number = dump.l1_batch_number();
        tracing::error!("VM execution diverged on batch #{batch_number}!");
        dump.set_storage(VmStorageDump::new(&self.main_vm_storage, main_vm));
        (self.dump_handler)(dump);

        if self.panic_on_divergence {
            panic!("{err:?}");
        } else {
            tracing::error!("{err:#}");
            tracing::warn!(
                "New VM is dropped; following VM actions will be executed only on the main VM"
            );
        }
    }
}

#[derive(Debug)]
pub struct ShadowVm<S, Main, Shadow = vm_fast::Vm<ImmutableStorageView<S>>> {
    main: Main,
    shadow: RefCell<Option<VmWithReporting<S, Shadow>>>,
}

impl<S, Main, Shadow> ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmInterface,
    Shadow: VmInterface,
{
    pub fn set_dump_handler(&mut self, handler: VmDumpHandler) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.dump_handler = handler;
        }
    }

    pub(crate) fn set_panic_on_divergence(&mut self, panic: bool) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.panic_on_divergence = panic;
        }
    }
}

impl<S, Main, Shadow> VmFactory<StorageView<S>> for ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmFactory<StorageView<S>>,
    Shadow: VmFactory<StorageView<S>>,
{
    fn new(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Self {
        let main = Main::new(batch_env.clone(), system_env.clone(), storage.clone());
        let shadow = Shadow::new(batch_env.clone(), system_env.clone(), storage.clone());
        let shadow = VmWithReporting {
            vm: shadow,
            main_vm_storage: storage,
            partial_dump: VmDump::new(batch_env, system_env),
            dump_handler: Arc::new(drop), // We don't want to log the dump (it's too large), so there's no trivial way to handle it
            panic_on_divergence: true,
        };
        Self {
            main,
            shadow: RefCell::new(Some(shadow)),
        }
    }
}

/// **Important.** This doesn't properly handle tracers; they are not passed to the shadow VM!
impl<S, Main, Shadow> VmInterface for ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmInterface,
    Shadow: VmInterface,
{
    type TracerDispatcher = Main::TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.partial_dump.push_transaction(tx.clone());
            shadow.vm.push_transaction(tx.clone());
        }
        self.main.push_transaction(tx);
    }

    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        let main_result = self.main.execute(execution_mode);
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_result = shadow.vm.execute(execution_mode);
            let mut errors = DivergenceErrors::default();
            errors.check_results_match(&main_result, &shadow_result);
            if let Err(err) = errors.into_result() {
                let ctx = format!("executing VM with mode {execution_mode:?}");
                self.shadow
                    .take()
                    .unwrap()
                    .report(&self.main, err.context(ctx));
            }
        }
        main_result
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let main_result = self.main.inspect(dispatcher, execution_mode);
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_result = shadow
                .vm
                .inspect(Shadow::TracerDispatcher::default(), execution_mode);
            let mut errors = DivergenceErrors::default();
            errors.check_results_match(&main_result, &shadow_result);

            if let Err(err) = errors.into_result() {
                let ctx = format!("executing VM with mode {execution_mode:?}");
                self.shadow
                    .take()
                    .unwrap()
                    .report(&self.main, err.context(ctx));
            }
        }
        main_result
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        let main_memory = self.main.get_bootloader_memory();
        if let Some(shadow) = &*self.shadow.borrow() {
            let shadow_memory = shadow.vm.get_bootloader_memory();
            let result =
                DivergenceErrors::single("get_bootloader_memory", &main_memory, &shadow_memory);
            if let Err(err) = result {
                self.shadow.take().unwrap().report(&self.main, err);
            }
        }
        main_memory
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        let main_bytecodes = self.main.get_last_tx_compressed_bytecodes();
        if let Some(shadow) = &*self.shadow.borrow() {
            let shadow_bytecodes = shadow.vm.get_last_tx_compressed_bytecodes();
            let result = DivergenceErrors::single(
                "get_last_tx_compressed_bytecodes",
                &main_bytecodes,
                &shadow_bytecodes,
            );
            if let Err(err) = result {
                self.shadow.take().unwrap().report(&self.main, err);
            }
        }
        main_bytecodes
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.main.start_new_l2_block(l2_block_env);
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.partial_dump.push_block(l2_block_env);
            shadow.vm.start_new_l2_block(l2_block_env);
        }
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        let main_state = self.main.get_current_execution_state();
        if let Some(shadow) = &*self.shadow.borrow() {
            let shadow_state = shadow.vm.get_current_execution_state();
            let result =
                DivergenceErrors::single("get_current_execution_state", &main_state, &shadow_state);
            if let Err(err) = result {
                self.shadow.take().unwrap().report(&self.main, err);
            }
        }
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
        let tx_hash = tx.hash();
        let main_result = self
            .main
            .execute_transaction_with_bytecode_compression(tx.clone(), with_compression);
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.partial_dump.push_transaction(tx.clone());
            let shadow_result = shadow
                .vm
                .execute_transaction_with_bytecode_compression(tx, with_compression);
            let mut errors = DivergenceErrors::default();
            errors.check_results_match(&main_result.1, &shadow_result.1);
            if let Err(err) = errors.into_result() {
                let ctx = format!(
                    "executing transaction {tx_hash:?}, with_compression={with_compression:?}"
                );
                self.shadow
                    .take()
                    .unwrap()
                    .report(&self.main, err.context(ctx));
            }
        }
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
        let tx_hash = tx.hash();
        let main_result = self.main.inspect_transaction_with_bytecode_compression(
            tracer,
            tx.clone(),
            with_compression,
        );
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.partial_dump.push_transaction(tx.clone());
            let shadow_result = shadow.vm.inspect_transaction_with_bytecode_compression(
                Shadow::TracerDispatcher::default(),
                tx,
                with_compression,
            );
            let mut errors = DivergenceErrors::default();
            errors.check_results_match(&main_result.1, &shadow_result.1);
            if let Err(err) = errors.into_result() {
                let ctx = format!(
                    "inspecting transaction {tx_hash:?}, with_compression={with_compression:?}"
                );
                self.shadow
                    .take()
                    .unwrap()
                    .report(&self.main, err.context(ctx));
            }
        }
        main_result
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        self.main.record_vm_memory_metrics()
    }

    fn gas_remaining(&self) -> u32 {
        let main_gas = self.main.gas_remaining();
        if let Some(shadow) = &*self.shadow.borrow() {
            let shadow_gas = shadow.vm.gas_remaining();
            let result = DivergenceErrors::single("gas_remaining", &main_gas, &shadow_gas);
            if let Err(err) = result {
                self.shadow.take().unwrap().report(&self.main, err);
            }
        }
        main_gas
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let main_batch = self.main.finish_batch();
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_batch = shadow.vm.finish_batch();
            let mut errors = DivergenceErrors::default();
            errors.check_results_match(
                &main_batch.block_tip_execution_result,
                &shadow_batch.block_tip_execution_result,
            );
            errors.check_final_states_match(
                &main_batch.final_execution_state,
                &shadow_batch.final_execution_state,
            );
            errors.check_match(
                "final_bootloader_memory",
                &main_batch.final_bootloader_memory,
                &shadow_batch.final_bootloader_memory,
            );
            errors.check_match(
                "pubdata_input",
                &main_batch.pubdata_input,
                &shadow_batch.pubdata_input,
            );
            errors.check_match(
                "state_diffs",
                &main_batch.state_diffs,
                &shadow_batch.state_diffs,
            );

            if let Err(err) = errors.into_result() {
                self.shadow.take().unwrap().report(&self.main, err);
            }
        }
        main_batch
    }
}

#[must_use = "Should be converted to a `Result`"]
#[derive(Debug, Default)]
pub struct DivergenceErrors(Vec<anyhow::Error>);

impl DivergenceErrors {
    fn single<T: fmt::Debug + PartialEq>(
        context: &str,
        main: &T,
        shadow: &T,
    ) -> anyhow::Result<()> {
        let mut this = Self::default();
        this.check_match(context, main, shadow);
        this.into_result()
    }

    fn check_results_match(
        &mut self,
        main_result: &VmExecutionResultAndLogs,
        shadow_result: &VmExecutionResultAndLogs,
    ) {
        self.check_match("result", &main_result.result, &shadow_result.result);
        self.check_match(
            "logs.events",
            &main_result.logs.events,
            &shadow_result.logs.events,
        );
        self.check_match(
            "logs.system_l2_to_l1_logs",
            &main_result.logs.system_l2_to_l1_logs,
            &shadow_result.logs.system_l2_to_l1_logs,
        );
        self.check_match(
            "logs.user_l2_to_l1_logs",
            &main_result.logs.user_l2_to_l1_logs,
            &shadow_result.logs.user_l2_to_l1_logs,
        );
        let main_logs = UniqueStorageLogs::new(&main_result.logs.storage_logs);
        let shadow_logs = UniqueStorageLogs::new(&shadow_result.logs.storage_logs);
        self.check_match("logs.storage_logs", &main_logs, &shadow_logs);
        self.check_match("refunds", &main_result.refunds, &shadow_result.refunds);
    }

    fn check_match<T: fmt::Debug + PartialEq>(&mut self, context: &str, main: &T, shadow: &T) {
        if main != shadow {
            let comparison = pretty_assertions::Comparison::new(main, shadow);
            let err = anyhow::anyhow!("`{context}` mismatch: {comparison}");
            self.0.push(err);
        }
    }

    fn check_final_states_match(
        &mut self,
        main: &CurrentExecutionState,
        shadow: &CurrentExecutionState,
    ) {
        self.check_match("final_state.events", &main.events, &shadow.events);
        self.check_match(
            "final_state.user_l2_to_l1_logs",
            &main.user_l2_to_l1_logs,
            &shadow.user_l2_to_l1_logs,
        );
        self.check_match(
            "final_state.system_logs",
            &main.system_logs,
            &shadow.system_logs,
        );
        self.check_match(
            "final_state.storage_refunds",
            &main.storage_refunds,
            &shadow.storage_refunds,
        );
        self.check_match(
            "final_state.pubdata_costs",
            &main.pubdata_costs,
            &shadow.pubdata_costs,
        );
        self.check_match(
            "final_state.used_contract_hashes",
            &main.used_contract_hashes.iter().collect::<BTreeSet<_>>(),
            &shadow.used_contract_hashes.iter().collect::<BTreeSet<_>>(),
        );

        let main_deduplicated_logs = Self::gather_logs(&main.deduplicated_storage_logs);
        let shadow_deduplicated_logs = Self::gather_logs(&shadow.deduplicated_storage_logs);
        self.check_match(
            "deduplicated_storage_logs",
            &main_deduplicated_logs,
            &shadow_deduplicated_logs,
        );
    }

    fn gather_logs(logs: &[StorageLog]) -> BTreeMap<StorageKey, &StorageLog> {
        logs.iter()
            .filter(|log| log.is_write())
            .map(|log| (log.key, log))
            .collect()
    }

    fn into_result(self) -> anyhow::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "divergence between old VM and new VM execution: [{:?}]",
                self.0
            ))
        }
    }
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

        // Remove no-op write logs (i.e., X -> X writes) produced by the old VM.
        unique_logs.retain(|_, log| log.previous_value != log.log.value);
        Self(unique_logs)
    }
}

impl<S, T> VmInterfaceHistoryEnabled for ShadowVm<S, T>
where
    S: ReadStorage,
    T: VmInterfaceHistoryEnabled,
{
    fn make_snapshot(&mut self) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.vm.make_snapshot();
        }
        self.main.make_snapshot();
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.vm.rollback_to_the_latest_snapshot();
        }
        self.main.rollback_to_the_latest_snapshot();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.vm.pop_snapshot_no_rollback();
        }
        self.main.pop_snapshot_no_rollback();
    }
}
