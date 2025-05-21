//! Implementation of "executing" methods, e.g. `eth_call`.

use std::{
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::{runtime::Handle, sync::Mutex};
use zksync_dal::{Connection, Core};
use zksync_multivm::{
    interface::{
        executor::{OneshotExecutor, TransactionValidator},
        storage::{ReadStorage, StorageWithOverrides},
        tracer::TimestampAsserterParams,
        utils::{DivergenceHandler, VmDump},
        Call, DeduplicatedWritesMetrics, ExecutionResult, OneshotEnv, OneshotTracingParams,
        TransactionExecutionMetrics, TxExecutionArgs, VmEvent,
    },
    utils::StorageWritesDeduplicator,
};
use zksync_object_store::{Bucket, ObjectStore};
use zksync_state::{PostgresStorage, PostgresStorageCaches};
use zksync_types::{
    api::state_override::StateOverride, fee_model::BatchFeeInput, l2::L2Tx, vm::FastVmMode,
    StorageLog, Transaction,
};
use zksync_vm_executor::oneshot::{MainOneshotExecutor, MockOneshotExecutor};

use super::{vm_metrics::SandboxStage, BlockArgs, VmPermit, SANDBOX_METRICS};
#[cfg(test)]
use crate::execution_sandbox::testonly;
use crate::{execution_sandbox::storage::apply_state_override, tx_sender::SandboxExecutorOptions};

/// Action that can be executed by [`SandboxExecutor`].
#[derive(Debug)]
pub(crate) enum SandboxAction {
    /// Execute a transaction.
    Execution { tx: L2Tx, fee_input: BatchFeeInput },
    /// Execute a call, possibly with tracing.
    Call {
        call: L2Tx,
        fee_input: BatchFeeInput,
        enforced_base_fee: Option<u64>,
        tracing_params: OneshotTracingParams,
    },
    /// Estimate gas for a transaction.
    GasEstimation {
        tx: Transaction,
        fee_input: BatchFeeInput,
        base_fee: u64,
    },
}

impl SandboxAction {
    fn into_parts(self) -> (TxExecutionArgs, OneshotTracingParams) {
        match self {
            Self::Execution { tx, .. } => (
                TxExecutionArgs::for_validation(tx),
                OneshotTracingParams::default(),
            ),
            Self::GasEstimation { tx, .. } => (
                TxExecutionArgs::for_gas_estimate(tx),
                OneshotTracingParams::default(),
            ),
            Self::Call {
                call,
                tracing_params,
                ..
            } => (TxExecutionArgs::for_eth_call(call), tracing_params),
        }
    }
}

/// Output of [`SandboxExecutor::execute_in_sandbox()`].
#[derive(Debug, Clone)]
pub struct SandboxExecutionOutput {
    /// Output of the VM.
    pub result: ExecutionResult,
    /// Write logs produced by the VM.
    pub write_logs: Vec<StorageLog>,
    /// Events produced by the VM.
    pub events: Vec<VmEvent>,
    /// Traced calls if requested.
    pub call_traces: Vec<Call>,
    /// Execution metrics.
    pub metrics: TransactionExecutionMetrics,
    /// Were published bytecodes OK?
    pub are_published_bytecodes_ok: bool,
}

impl SandboxExecutionOutput {
    pub fn mock_success() -> Self {
        Self {
            result: ExecutionResult::Success { output: Vec::new() },
            write_logs: Vec::new(),
            events: Vec::new(),
            call_traces: Vec::new(),
            metrics: TransactionExecutionMetrics {
                writes: DeduplicatedWritesMetrics::default(),
                vm: Default::default(),
                gas_remaining: 0,
                gas_refunded: 0,
            },
            are_published_bytecodes_ok: true,
        }
    }
}

pub(super) type SandboxStorage = StorageWithOverrides<PostgresStorage<'static>>;

/// Higher-level wrapper around a oneshot VM executor used in the API server.
#[async_trait]
pub(crate) trait SandboxExecutorEngine<S: ReadStorage = SandboxStorage>:
    Send + Sync + fmt::Debug + TransactionValidator<S>
{
    async fn execute_in_sandbox(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<SandboxExecutionOutput>;
}

#[async_trait]
impl<S, T> SandboxExecutorEngine<S> for T
where
    S: ReadStorage + Send + 'static,
    T: OneshotExecutor<S> + TransactionValidator<S> + Send + Sync + fmt::Debug,
{
    async fn execute_in_sandbox(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<SandboxExecutionOutput> {
        let result = self
            .inspect_transaction_with_bytecode_compression(storage, env, args, tracing_params)
            .await?;
        let tx_result = result.tx_result;
        let metrics = TransactionExecutionMetrics {
            writes: StorageWritesDeduplicator::apply_on_empty_state(&tx_result.logs.storage_logs),
            vm: tx_result.get_execution_metrics(),
            gas_remaining: tx_result.statistics.gas_remaining,
            gas_refunded: tx_result.refunds.gas_refunded,
        };

        let storage_logs = tx_result.logs.storage_logs;
        Ok(SandboxExecutionOutput {
            result: tx_result.result,
            write_logs: storage_logs
                .into_iter()
                .filter_map(|log| log.log.is_write().then_some(log.log))
                .collect(),
            events: tx_result.logs.events,
            call_traces: result.call_traces,
            metrics,
            are_published_bytecodes_ok: result.compression_result.is_ok(),
        })
    }
}

/// Executor of transactions / calls used in the API server.
#[derive(Debug)]
pub(crate) struct SandboxExecutor {
    pub(super) engine: Box<dyn SandboxExecutorEngine>,
    pub(super) options: SandboxExecutorOptions,
    storage_caches: Option<PostgresStorageCaches>,
    pub(super) timestamp_asserter_params: Option<TimestampAsserterParams>,
    vm_divergence_counter: Arc<AtomicUsize>,
}

impl SandboxExecutor {
    pub fn real(
        options: SandboxExecutorOptions,
        caches: PostgresStorageCaches,
        missed_storage_invocation_limit: usize,
        timestamp_asserter_params: Option<TimestampAsserterParams>,
    ) -> Self {
        let mut executor = MainOneshotExecutor::new(missed_storage_invocation_limit);
        executor.set_fast_vm_mode(options.fast_vm_mode);

        let vm_divergence_counter = Arc::<AtomicUsize>::default();
        if cfg!(test) {
            // Panic on divergence in order to fail the running test.
            executor.set_divergence_handler(DivergenceHandler::new(|err, _| {
                panic!("{err}");
            }));
        } else {
            let rt_handle = Handle::current();
            let store = options.vm_dump_store.clone();
            let vm_divergence_counter_for_handler = vm_divergence_counter.clone();
            let last_dump_timestamp = Mutex::new(None);

            executor.set_divergence_handler(DivergenceHandler::new(move |_, vm_dump| {
                vm_divergence_counter_for_handler.fetch_add(1, Ordering::Relaxed);
                if let Some(store) = &store {
                    if let Err(err) = rt_handle.block_on(Self::dump_vm_state(
                        &last_dump_timestamp,
                        store.as_ref(),
                        vm_dump,
                    )) {
                        tracing::error!("Saving VM dump failed: {err:#}");
                    }
                }
            }));
        }

        executor
            .set_execution_latency_histogram(&SANDBOX_METRICS.sandbox[&SandboxStage::Execution]);

        #[cfg(test)]
        let engine: Box<dyn SandboxExecutorEngine> =
            if let Some(storage_delay) = options.storage_delay {
                Box::new(testonly::SlowExecutor::new(executor, storage_delay))
            } else {
                Box::new(executor)
            };
        #[cfg(not(test))]
        let engine = Box::new(executor);

        Self {
            engine,
            options,
            storage_caches: Some(caches),
            timestamp_asserter_params,
            vm_divergence_counter,
        }
    }

    pub(crate) fn vm_mode(&self) -> FastVmMode {
        self.options.fast_vm_mode
    }

    pub(crate) fn vm_divergence_counter(&self) -> Arc<AtomicUsize> {
        self.vm_divergence_counter.clone()
    }

    async fn dump_vm_state(
        last_dump_timestamp: &Mutex<Option<Instant>>,
        store: &dyn ObjectStore,
        vm_dump: VmDump,
    ) -> anyhow::Result<()> {
        /// Minimum interval between VM dumps.
        const DUMP_INTERVAL: Duration = Duration::from_secs(600); // 10 minutes

        let mut last_dump_timestamp = last_dump_timestamp.lock().await;
        let now = Instant::now();
        let should_dump = last_dump_timestamp.is_none_or(|ts| {
            now.checked_duration_since(ts)
                .is_some_and(|elapsed| elapsed >= DUMP_INTERVAL)
        });
        if !should_dump {
            return Ok(());
        }
        *last_dump_timestamp = Some(now);
        drop(last_dump_timestamp); // We don't need to hold a lock any longer, in particular across the `await` point below

        let ts = SystemTime::UNIX_EPOCH
            .elapsed()
            .context("invalid wall clock time")?
            .as_millis();
        let dump_filename = format!("shadow_vm_dump_api_{ts}.json");
        let vm_dump = serde_json::to_vec(&vm_dump).context("failed serializing VM dump")?;
        store
            .put_raw(Bucket::VmDumps, &dump_filename, vm_dump)
            .await
            .context("failed saving VM dump to object store")?;
        tracing::info!(
            dump_filename,
            "Saved VM dump for diverging VM execution in object store"
        );
        Ok(())
    }

    pub(crate) async fn mock(executor: MockOneshotExecutor) -> Self {
        Self::custom_mock(executor, SandboxExecutorOptions::mock().await)
    }

    pub(crate) fn custom_mock(
        executor: MockOneshotExecutor,
        options: SandboxExecutorOptions,
    ) -> Self {
        Self {
            engine: Box::new(executor),
            options,
            storage_caches: None,
            timestamp_asserter_params: None,
            vm_divergence_counter: Arc::default(),
        }
    }

    /// This method assumes that (block with number `resolved_block_number` is present in DB)
    /// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
    pub async fn execute_in_sandbox(
        &self,
        _vm_permit: VmPermit,
        connection: Connection<'static, Core>,
        action: SandboxAction,
        block_args: &BlockArgs,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<SandboxExecutionOutput> {
        let (env, storage) = self
            .prepare_env_and_storage(connection, block_args, &action)
            .await?;

        let storage = if let Some(state_override) = state_override {
            tokio::task::spawn_blocking(|| apply_state_override(storage, state_override))
                .await
                .context("applying state override panicked")?
        } else {
            // Do not spawn a new thread in the most frequent case.
            StorageWithOverrides::new(storage)
        };

        let (execution_args, tracing_params) = action.into_parts();
        self.engine
            .execute_in_sandbox(storage, env, execution_args, tracing_params)
            .await
    }

    pub(super) async fn prepare_env_and_storage(
        &self,
        mut connection: Connection<'static, Core>,
        block_args: &BlockArgs,
        action: &SandboxAction,
    ) -> anyhow::Result<(OneshotEnv, PostgresStorage<'static>)> {
        let initialization_stage = SANDBOX_METRICS.sandbox[&SandboxStage::Initialization].start();
        let resolve_started_at = Instant::now();
        let resolve_time = resolve_started_at.elapsed();
        let resolved_block_info = &block_args.resolved;
        // We don't want to emit too many logs.
        if resolve_time > Duration::from_millis(10) {
            tracing::debug!("Resolved block numbers (took {resolve_time:?})");
        }

        let env = match action {
            SandboxAction::Execution { fee_input, tx } => {
                self.options
                    .eth_call
                    .to_execute_env(&mut connection, resolved_block_info, *fee_input, tx)
                    .await?
            }
            &SandboxAction::Call {
                fee_input,
                enforced_base_fee,
                ..
            } => {
                self.options
                    .eth_call
                    .to_call_env(
                        &mut connection,
                        resolved_block_info,
                        fee_input,
                        enforced_base_fee,
                    )
                    .await?
            }
            &SandboxAction::GasEstimation {
                fee_input,
                base_fee,
                ..
            } => {
                self.options
                    .estimate_gas
                    .to_env(&mut connection, resolved_block_info, fee_input, base_fee)
                    .await?
            }
        };

        if block_args.resolves_to_latest_sealed_l2_block() {
            if let Some(caches) = &self.storage_caches {
                caches.schedule_values_update(resolved_block_info.state_l2_block_number());
            }
        }

        let mut storage = PostgresStorage::new_async(
            Handle::current(),
            connection,
            resolved_block_info.state_l2_block_number(),
            false,
        )
        .await
        .context("cannot create `PostgresStorage`")?;

        if let Some(caches) = &self.storage_caches {
            storage = storage.with_caches(caches.clone());
        }
        initialization_stage.observe();
        Ok((env, storage))
    }
}
