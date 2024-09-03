use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use zksync_types::{StorageKey, StorageLog, StorageLogWithPreviousValue, Transaction};

use super::dump::{DumpingVm, VmDump};
use crate::{
    storage::{ReadStorage, StoragePtr, StorageView},
    BytecodeCompressionResult, CurrentExecutionState, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
    SystemEnv, VmExecutionMode, VmExecutionResultAndLogs, VmFactory, VmInterface,
    VmInterfaceHistoryEnabled, VmMemoryMetrics, VmTrackingContracts,
};

/// Handler for VM divergences.
#[derive(Clone)]
pub struct DivergenceHandler(Arc<dyn Fn(DivergenceErrors, VmDump) + Send + Sync>);

impl fmt::Debug for DivergenceHandler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("DivergenceHandler")
            .field(&"_")
            .finish()
    }
}

/// Default handler that panics.
impl Default for DivergenceHandler {
    fn default() -> Self {
        Self(Arc::new(|err, _| {
            // There's no easy way to output the VM dump; it's too large to be logged.
            panic!("{err}");
        }))
    }
}

impl DivergenceHandler {
    /// Creates a new handler from the provided closure.
    pub fn new(f: impl Fn(DivergenceErrors, VmDump) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    fn handle(&self, err: DivergenceErrors, dump: VmDump) {
        self.0(err, dump);
    }
}

#[derive(Debug)]
struct VmWithReporting<Shadow> {
    vm: Shadow,
    divergence_handler: DivergenceHandler,
}

impl<Shadow: VmInterface> VmWithReporting<Shadow> {
    fn report(self, err: DivergenceErrors, dump: VmDump) {
        tracing::error!("{err}");
        self.divergence_handler.handle(err, dump);
        tracing::warn!(
            "New VM is dropped; following VM actions will be executed only on the main VM"
        );
    }
}

/// Shadowed VM that executes 2 VMs for each operation and compares their outputs.
///
/// If a divergence is detected, the VM state is dumped using [a pluggable handler](Self::set_dump_handler()),
/// after which the VM drops the shadowed VM (since it's assumed that its state can contain arbitrary garbage at this point).
#[derive(Debug)]
pub struct ShadowVm<S, Main, Shadow> {
    main: DumpingVm<S, Main>,
    shadow: RefCell<Option<VmWithReporting<Shadow>>>,
}

impl<S, Main, Shadow> ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmTrackingContracts,
    Shadow: VmInterface,
{
    /// Sets the divergence handler to be used by this VM.
    pub fn set_divergence_handler(&mut self, handler: DivergenceHandler) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.divergence_handler = handler;
        }
    }

    /// Mutable ref is not necessary, but it automatically drops potential borrows.
    fn report(&mut self, err: DivergenceErrors) {
        self.report_shared(err);
    }

    /// The caller is responsible for dropping any `shadow` borrows beforehand.
    fn report_shared(&self, err: DivergenceErrors) {
        self.shadow
            .take()
            .unwrap()
            .report(err, self.main.dump_state());
    }

    /// Dumps the current VM state.
    pub fn dump_state(&self) -> VmDump {
        self.main.dump_state()
    }
}

impl<S, Main, Shadow> ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmFactory<StorageView<S>> + VmTrackingContracts,
    Shadow: VmInterface,
{
    /// Creates a VM with a custom shadow storage.
    pub fn with_custom_shadow<ShadowS>(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
        shadow_storage: StoragePtr<ShadowS>,
    ) -> Self
    where
        Shadow: VmFactory<ShadowS>,
    {
        let main = DumpingVm::new(batch_env.clone(), system_env.clone(), storage.clone());
        let shadow = Shadow::new(batch_env.clone(), system_env.clone(), shadow_storage);
        let shadow = VmWithReporting {
            vm: shadow,
            divergence_handler: DivergenceHandler::default(),
        };
        Self {
            main,
            shadow: RefCell::new(Some(shadow)),
        }
    }
}

impl<S, Main, Shadow> VmFactory<StorageView<S>> for ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmFactory<StorageView<S>> + VmTrackingContracts,
    Shadow: VmFactory<StorageView<S>>,
{
    fn new(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Self {
        Self::with_custom_shadow(batch_env, system_env, storage.clone(), storage)
    }
}

/// **Important.** This doesn't properly handle tracers; they are not passed to the shadow VM!
impl<S, Main, Shadow> VmInterface for ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmTrackingContracts,
    Shadow: VmInterface,
{
    type TracerDispatcher = <Main as VmInterface>::TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.vm.push_transaction(tx.clone());
        }
        self.main.push_transaction(tx);
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
            let mut errors = DivergenceErrors::new();
            errors.check_results_match(&main_result, &shadow_result);

            if let Err(err) = errors.into_result() {
                let ctx = format!("executing VM with mode {execution_mode:?}");
                self.report(err.context(ctx));
            }
        }
        main_result
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.main.start_new_l2_block(l2_block_env);
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.vm.start_new_l2_block(l2_block_env);
        }
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult, VmExecutionResultAndLogs) {
        let tx_hash = tx.hash();
        let main_result = self.main.inspect_transaction_with_bytecode_compression(
            tracer,
            tx.clone(),
            with_compression,
        );
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_result = shadow.vm.inspect_transaction_with_bytecode_compression(
                Shadow::TracerDispatcher::default(),
                tx,
                with_compression,
            );
            let mut errors = DivergenceErrors::new();
            errors.check_results_match(&main_result.1, &shadow_result.1);
            if let Err(err) = errors.into_result() {
                let ctx = format!(
                    "inspecting transaction {tx_hash:?}, with_compression={with_compression:?}"
                );
                self.report(err.context(ctx));
            }
        }
        main_result
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        self.main.record_vm_memory_metrics()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let main_batch = self.main.finish_batch();
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_batch = shadow.vm.finish_batch();
            let mut errors = DivergenceErrors::new();
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
                self.report(err);
            }
        }
        main_batch
    }
}

#[derive(Debug)]
pub struct DivergenceErrors {
    divergences: Vec<String>,
    context: Option<String>,
}

impl fmt::Display for DivergenceErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(context) = &self.context {
            write!(
                formatter,
                "VM execution diverged: {context}: [{}]",
                self.divergences.join(", ")
            )
        } else {
            write!(
                formatter,
                "VM execution diverged: [{}]",
                self.divergences.join(", ")
            )
        }
    }
}

impl DivergenceErrors {
    fn new() -> Self {
        Self {
            divergences: vec![],
            context: None,
        }
    }

    fn context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
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
        self.check_match(
            "gas_remaining",
            &main_result.statistics.gas_remaining,
            &shadow_result.statistics.gas_remaining,
        );
    }

    fn check_match<T: fmt::Debug + PartialEq>(&mut self, context: &str, main: &T, shadow: &T) {
        if main != shadow {
            let comparison = pretty_assertions::Comparison::new(main, shadow);
            let err = format!("`{context}` mismatch: {comparison}");
            self.divergences.push(err);
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

    fn into_result(self) -> Result<(), Self> {
        if self.divergences.is_empty() {
            Ok(())
        } else {
            Err(self)
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

impl<S, Main, Shadow> VmInterfaceHistoryEnabled for ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmInterfaceHistoryEnabled + VmTrackingContracts,
    Shadow: VmInterfaceHistoryEnabled,
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
