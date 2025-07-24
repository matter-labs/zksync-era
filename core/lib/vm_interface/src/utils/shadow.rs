use std::{
    any,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    rc::Rc,
    sync::Arc,
};

use zksync_types::{
    Address, StorageKey, StorageLog, StorageLogWithPreviousValue, Transaction, U256,
};

use super::dump::{DumpingVm, VmDump};
use crate::{
    pubdata::PubdataBuilder,
    storage::{ReadStorage, StoragePtr, StorageView},
    tracer::{ValidationError, ValidationTraces, ViolatedValidationRule},
    BytecodeCompressionResult, Call, CallType, CurrentExecutionState, ExecutionResult,
    FinishedL1Batch, Halt, InspectExecutionMode, L1BatchEnv, L2BlockEnv, PushTransactionResult,
    SystemEnv, VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
    VmTrackingContracts,
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

    /// Handles a VM divergence.
    pub fn handle(&self, err: DivergenceErrors, dump: VmDump) {
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

/// Reference to either the main or shadow VM.
#[derive(Debug)]
pub enum ShadowRef<'a, Main, Shadow> {
    /// Reference to the main VM.
    Main(&'a Main),
    /// Reference to the shadow VM.
    Shadow(&'a Shadow),
}

/// Mutable reference to either the main or shadow VM.
#[derive(Debug)]
pub enum ShadowMut<'a, Main, Shadow> {
    /// Reference to the main VM.
    Main(&'a mut Main),
    /// Reference to the shadow VM.
    Shadow(&'a mut Shadow),
}

/// Type that can check divergence between its instances.
pub trait CheckDivergence {
    /// Checks divergences and returns a list of divergence errors, if any.
    fn check_divergence(&self, other: &Self) -> DivergenceErrors;
}

#[derive(Debug)]
struct DivergingEq<T>(T);

impl<T: fmt::Debug + PartialEq + 'static> CheckDivergence for DivergingEq<T> {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let mut errors = DivergenceErrors::new();
        errors.check_match(any::type_name::<T>(), &self.0, &other.0);
        errors
    }
}

impl CheckDivergence for CurrentExecutionState {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let mut errors = DivergenceErrors::new();
        errors.check_match("final_state.events", &self.events, &other.events);
        errors.check_match(
            "final_state.user_l2_to_l1_logs",
            &self.user_l2_to_l1_logs,
            &other.user_l2_to_l1_logs,
        );
        errors.check_match(
            "final_state.system_logs",
            &self.system_logs,
            &other.system_logs,
        );
        errors.check_match(
            "final_state.storage_refunds",
            &self.storage_refunds,
            &other.storage_refunds,
        );
        errors.check_match(
            "final_state.pubdata_costs",
            &self.pubdata_costs,
            &other.pubdata_costs,
        );
        errors.check_match(
            "final_state.used_contract_hashes",
            &self.used_contract_hashes.iter().collect::<BTreeSet<_>>(),
            &other.used_contract_hashes.iter().collect::<BTreeSet<_>>(),
        );

        let main_deduplicated_logs = DivergenceErrors::gather_logs(&self.deduplicated_storage_logs);
        let shadow_deduplicated_logs =
            DivergenceErrors::gather_logs(&other.deduplicated_storage_logs);
        errors.check_match(
            "deduplicated_storage_logs",
            &main_deduplicated_logs,
            &shadow_deduplicated_logs,
        );
        errors
    }
}

impl CheckDivergence for VmExecutionResultAndLogs {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let mut errors = DivergenceErrors::new();

        // Special case: the execution is stopped by a tracer. Because of significant differences between how tracers
        // work in legacy and fast VMs, we only require the general stop reason to be the same, and ignore anything else.
        if matches!(
            (&self.result, &other.result),
            (
                ExecutionResult::Halt {
                    reason: Halt::TracerCustom(_)
                },
                ExecutionResult::Halt {
                    reason: Halt::TracerCustom(_)
                }
            )
        ) {
            return errors;
        }

        errors.check_match("result", &self.result, &other.result);

        if matches!(
            &self.result,
            ExecutionResult::Halt {
                reason: Halt::ValidationOutOfGas,
            }
        ) {
            // Because of differences in how validation is implemented in the legacy and fast VMs, we don't require
            // the exact match in any other fields; even gas stats are slightly different.
            return errors;
        }

        errors.check_match("logs.events", &self.logs.events, &other.logs.events);
        errors.check_match(
            "logs.system_l2_to_l1_logs",
            &self.logs.system_l2_to_l1_logs,
            &other.logs.system_l2_to_l1_logs,
        );
        errors.check_match(
            "logs.user_l2_to_l1_logs",
            &self.logs.user_l2_to_l1_logs,
            &other.logs.user_l2_to_l1_logs,
        );
        let main_logs = UniqueStorageLogs::new(&self.logs.storage_logs);
        let shadow_logs = UniqueStorageLogs::new(&other.logs.storage_logs);
        errors.check_match("logs.storage_logs", &main_logs, &shadow_logs);
        errors.check_match("refunds", &self.refunds, &other.refunds);
        errors.check_match(
            "statistics.circuit_statistic",
            &self.statistics.circuit_statistic,
            &other.statistics.circuit_statistic,
        );
        errors.check_match(
            "statistics.pubdata_published",
            &self.statistics.pubdata_published,
            &other.statistics.pubdata_published,
        );
        errors.check_match(
            "statistics.gas_remaining",
            &self.statistics.gas_remaining,
            &other.statistics.gas_remaining,
        );
        errors.check_match(
            "statistics.gas_used",
            &self.statistics.gas_used,
            &other.statistics.gas_used,
        );
        errors.check_match(
            "statistics.computational_gas_used",
            &self.statistics.computational_gas_used,
            &other.statistics.computational_gas_used,
        );

        // Order deps to have a more reasonable diff on a mismatch
        let these_deps = self.dynamic_factory_deps.iter().collect::<BTreeMap<_, _>>();
        let other_deps = other
            .dynamic_factory_deps
            .iter()
            .collect::<BTreeMap<_, _>>();
        errors.check_match("dynamic_factory_deps", &these_deps, &other_deps);
        errors
    }
}

impl CheckDivergence for FinishedL1Batch {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let mut errors = DivergenceErrors::new();
        errors.extend(
            self.block_tip_execution_result
                .check_divergence(&other.block_tip_execution_result),
        );
        errors.extend(
            self.final_execution_state
                .check_divergence(&other.final_execution_state),
        );

        errors.check_match(
            "final_bootloader_memory",
            &self.final_bootloader_memory,
            &other.final_bootloader_memory,
        );
        errors.check_match("pubdata_input", &self.pubdata_input, &other.pubdata_input);
        errors.check_match("state_diffs", &self.state_diffs, &other.state_diffs);
        errors
    }
}

impl CheckDivergence for Result<ValidationTraces, ValidationError> {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let mut errors = DivergenceErrors::new();

        if matches!(
            (self, other),
            (
                Err(ValidationError::ViolatedRule(
                    ViolatedValidationRule::TookTooManyComputationalGas(_)
                )),
                Err(ValidationError::ViolatedRule(
                    ViolatedValidationRule::TookTooManyComputationalGas(_)
                )),
            )
        ) {
            // Because of differences in how validation is implemented in the legacy and fast VMs, we don't require
            // the exact match in the computational gas limit reported; it only influences error messages.
            return errors;
        }

        errors.check_match("validation result", self, other);
        errors
    }
}

/// `PartialEq` for `Call` doesn't compare gas-related fields. Here, we do compare them.
#[derive(Debug, PartialEq)]
struct StrictCall<'a> {
    r#type: CallType,
    from: Address,
    to: Address,
    // `gas` / `parent_gas` differ between fast VM and legacy VM during validation
    gas_used: u64,
    value: U256,
    input: &'a [u8],
    output: &'a [u8],
    error: Option<&'a str>,
    revert_reason: Option<&'a str>,
}

impl<'a> StrictCall<'a> {
    fn flatten(calls: &'a [Call]) -> Vec<Self> {
        let mut flattened = Vec::new();
        Self::flatten_inner(&mut flattened, calls);
        flattened
    }

    fn flatten_inner(flattened: &mut Vec<Self>, calls: &'a [Call]) {
        // Depth-first, parents-before-children traversal.
        for call in calls {
            flattened.push(Self {
                r#type: call.r#type,
                from: call.from,
                to: call.to,
                gas_used: call.gas_used,
                value: call.value,
                input: &call.input,
                output: &call.output,
                error: call.error.as_deref(),
                revert_reason: call.revert_reason.as_deref(),
            });
            Self::flatten_inner(flattened, &call.calls);
        }
    }
}

impl CheckDivergence for [Call] {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        let this = StrictCall::flatten(self);
        let other = StrictCall::flatten(other);
        let mut errors = DivergenceErrors::new();

        errors.check_match("call_traces", &this, &other);
        errors
    }
}

impl<T: CheckDivergence + ?Sized> CheckDivergence for &T {
    fn check_divergence(&self, other: &Self) -> DivergenceErrors {
        (**self).check_divergence(*other)
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
{
    /// Sets the divergence handler to be used by this VM.
    pub fn set_divergence_handler(&mut self, handler: DivergenceHandler) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.divergence_handler = handler;
        }
    }

    /// Dumps the current VM state.
    pub fn dump_state(&self) -> VmDump {
        self.main.dump_state()
    }

    /// Accesses the shadow VM if it's available (the instance is dropped after finding a divergence).
    pub fn shadow_mut(&mut self) -> Option<&mut Shadow> {
        Some(&mut self.shadow.get_mut().as_mut()?.vm)
    }
}

impl<S, Main, Shadow> ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmTrackingContracts,
    Shadow: VmInterface,
{
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

    /// Gets the specified value from both the main and shadow VM, checking whether it matches on both.
    pub fn get<R>(&self, name: &str, mut action: impl FnMut(ShadowRef<'_, Main, Shadow>) -> R) -> R
    where
        R: PartialEq + fmt::Debug + 'static,
    {
        self.get_custom(name, |r| DivergingEq(action(r))).0
    }

    /// Same as [`Self::get()`], but uses custom divergence checks for the type encapsulated in the [`CheckDivergence`] trait.
    pub fn get_custom<R: CheckDivergence>(
        &self,
        name: &str,
        mut action: impl FnMut(ShadowRef<'_, Main, Shadow>) -> R,
    ) -> R {
        let main_output = action(ShadowRef::Main(self.main.as_ref()));
        let borrow = self.shadow.borrow();
        if let Some(shadow) = &*borrow {
            let shadow_output = action(ShadowRef::Shadow(&shadow.vm));
            let errors = main_output.check_divergence(&shadow_output);
            if let Err(err) = errors.into_result() {
                drop(borrow);
                self.report_shared(err.context(format!("get({name})")));
            }
        }
        main_output
    }

    /// Gets the specified value from both the main and shadow VM, potentially changing their state
    /// and checking whether the returned value matches.
    pub fn get_mut<R>(
        &mut self,
        name: &str,
        mut action: impl FnMut(ShadowMut<'_, Main, Shadow>) -> R,
    ) -> R
    where
        R: PartialEq + fmt::Debug + 'static,
    {
        self.get_custom_mut(name, |r| DivergingEq(action(r))).0
    }

    /// Same as [`Self::get_mut()`], but uses custom divergence checks for the type encapsulated in the [`CheckDivergence`] trait.
    pub fn get_custom_mut<R>(
        &mut self,
        name: &str,
        mut action: impl FnMut(ShadowMut<'_, Main, Shadow>) -> R,
    ) -> R
    where
        R: CheckDivergence,
    {
        let main_output = action(ShadowMut::Main(self.main.as_mut()));
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_output = action(ShadowMut::Shadow(&mut shadow.vm));
            let errors = main_output.check_divergence(&shadow_output);
            if let Err(err) = errors.into_result() {
                self.report_shared(err.context(format!("get_mut({name})")));
            }
        }
        main_output
    }
}

impl<S, Main, Shadow> ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmFactory<StorageView<S>> + VmTrackingContracts,
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
        let main = DumpingVm::new(batch_env.clone(), system_env.clone(), storage);
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

impl<S, Main, Shadow> VmInterface for ShadowVm<S, Main, Shadow>
where
    S: ReadStorage,
    Main: VmTrackingContracts,
    Shadow: VmInterface,
{
    type TracerDispatcher = (
        <Main as VmInterface>::TracerDispatcher,
        <Shadow as VmInterface>::TracerDispatcher,
    );

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_> {
        let main_result = self.main.push_transaction(tx.clone());
        // Extend lifetime to `'static` so that the result isn't mutably borrowed from the main VM.
        // Unfortunately, there's no way to express that this borrow is actually immutable, which would allow not extending the lifetime unless there's a divergence.
        let main_result: PushTransactionResult<'static> = PushTransactionResult {
            compressed_bytecodes: main_result.compressed_bytecodes.into_owned().into(),
        };

        if let Some(shadow) = self.shadow.get_mut() {
            let tx_repr = format!("{tx:?}"); // includes little data, so is OK to call proactively
            let shadow_result = shadow.vm.push_transaction(tx);

            let mut errors = DivergenceErrors::new();
            errors.check_match(
                "bytecodes",
                &main_result.compressed_bytecodes,
                &shadow_result.compressed_bytecodes,
            );
            if let Err(err) = errors.into_result() {
                let ctx = format!("pushing transaction {tx_repr}");
                self.report(err.context(ctx));
            }
        }
        main_result
    }

    fn inspect(
        &mut self,
        (main_tracer, shadow_tracer): &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let main_result = self.main.inspect(main_tracer, execution_mode);
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_result = shadow.vm.inspect(shadow_tracer, execution_mode);
            let errors = main_result.check_divergence(&shadow_result);
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
        (main_tracer, shadow_tracer): &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        let tx_repr = format!("{tx:?}"); // includes little data, so is OK to call proactively

        let (main_bytecodes_result, main_tx_result) =
            self.main.inspect_transaction_with_bytecode_compression(
                main_tracer,
                tx.clone(),
                with_compression,
            );
        // Extend lifetime to `'static` so that the result isn't mutably borrowed from the main VM.
        // Unfortunately, there's no way to express that this borrow is actually immutable, which would allow not extending the lifetime unless there's a divergence.
        let main_bytecodes_result =
            main_bytecodes_result.map(|bytecodes| bytecodes.into_owned().into());

        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_result = shadow.vm.inspect_transaction_with_bytecode_compression(
                shadow_tracer,
                tx,
                with_compression,
            );
            let errors = main_tx_result.check_divergence(&shadow_result.1);
            if let Err(err) = errors.into_result() {
                let ctx = format!(
                    "inspecting transaction {tx_repr}, with_compression={with_compression:?}"
                );
                self.report(err.context(ctx));
            }
        }
        (main_bytecodes_result, main_tx_result)
    }

    fn finish_batch(&mut self, pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        let main_batch = self.main.finish_batch(pubdata_builder.clone());
        if let Some(shadow) = self.shadow.get_mut() {
            let shadow_batch = shadow.vm.finish_batch(pubdata_builder);
            let errors = main_batch.check_divergence(&shadow_batch);
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

    /// Extends this instance from another set of errors.
    pub fn extend(&mut self, from: Self) {
        self.divergences.extend(from.divergences);
    }

    fn context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }

    fn check_match<T: fmt::Debug + PartialEq>(&mut self, context: &str, main: &T, shadow: &T) {
        if main != shadow {
            let comparison = pretty_assertions::Comparison::new(main, shadow);
            let err = format!("`{context}` mismatch: {comparison}");
            self.divergences.push(err);
        }
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

    fn pop_front_snapshot_no_rollback(&mut self) {
        if let Some(shadow) = self.shadow.get_mut() {
            shadow.vm.pop_front_snapshot_no_rollback();
        }
        self.main.pop_front_snapshot_no_rollback();
    }
}
