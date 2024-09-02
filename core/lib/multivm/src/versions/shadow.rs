use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

use anyhow::Context as _;
use zksync_types::{StorageKey, StorageLog, StorageLogWithPreviousValue, Transaction};

use crate::{
    interface::{
        storage::{ImmutableStorageView, ReadStorage, StoragePtr, StorageView},
        BytecodeCompressionResult, CurrentExecutionState, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
        SystemEnv, VmExecutionMode, VmExecutionResultAndLogs, VmFactory, VmInterface,
        VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    vm_fast,
};

#[derive(Debug)]
pub struct ShadowVm<S, T> {
    main: T,
    shadow: vm_fast::Vm<ImmutableStorageView<S>>,
}

impl<S, T> VmFactory<StorageView<S>> for ShadowVm<S, T>
where
    S: ReadStorage,
    T: VmFactory<StorageView<S>>,
{
    fn new(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Self {
        Self {
            main: T::new(batch_env.clone(), system_env.clone(), storage.clone()),
            shadow: vm_fast::Vm::new(batch_env, system_env, ImmutableStorageView::new(storage)),
        }
    }
}

impl<S, T> VmInterface for ShadowVm<S, T>
where
    S: ReadStorage,
    T: VmInterface,
{
    type TracerDispatcher = T::TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) {
        self.shadow.push_transaction(tx.clone());
        self.main.push_transaction(tx);
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        let shadow_result = self.shadow.inspect((), execution_mode);
        let main_result = self.main.inspect(dispatcher, execution_mode);
        let mut errors = DivergenceErrors::default();
        errors.check_results_match(&main_result, &shadow_result);
        errors
            .into_result()
            .with_context(|| format!("executing VM with mode {execution_mode:?}"))
            .unwrap();
        main_result
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.shadow.start_new_l2_block(l2_block_env);
        self.main.start_new_l2_block(l2_block_env);
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        let tx_hash = tx.hash();
        let main_result = self.main.inspect_transaction_with_bytecode_compression(
            tracer,
            tx.clone(),
            with_compression,
        );
        let shadow_result =
            self.shadow
                .inspect_transaction_with_bytecode_compression((), tx, with_compression);
        let mut errors = DivergenceErrors::default();
        errors.check_results_match(&main_result.1, &shadow_result.1);
        errors
            .into_result()
            .with_context(|| {
                format!("inspecting transaction {tx_hash:?}, with_compression={with_compression:?}")
            })
            .unwrap();
        main_result
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        self.main.record_vm_memory_metrics()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let main_batch = self.main.finish_batch();
        let shadow_batch = self.shadow.finish_batch();

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
        errors.into_result().unwrap();
        main_batch
    }
}

#[must_use = "Should be converted to a `Result`"]
#[derive(Debug, Default)]
pub struct DivergenceErrors(Vec<anyhow::Error>);

impl DivergenceErrors {
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
            &main.used_contract_hashes.iter().collect::<HashSet<_>>(),
            &shadow.used_contract_hashes.iter().collect::<HashSet<_>>(),
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
