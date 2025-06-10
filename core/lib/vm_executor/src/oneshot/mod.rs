//! Oneshot VM executor.
//!
//! # Overview
//!
//! The root type of this module is [`MainOneshotExecutor`], a "default" [`OneshotExecutor`] implementation.
//! In addition to it, the module provides [`OneshotEnvParameters`] and [`BlockInfo`] / [`ResolvedBlockInfo`],
//! which can be used to prepare environment for `MainOneshotExecutor` (i.e., a [`OneshotEnv`] instance).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use zksync_instrument::alloc::AllocationGuard;
use zksync_multivm::{
    interface::{
        executor::{OneshotExecutor, TransactionValidator},
        storage::{ReadStorage, StoragePtr, StorageView, StorageWithOverrides, WriteStorage},
        tracer::{ValidationError, ValidationParams, ValidationTraces},
        utils::{DivergenceHandler, ShadowMut, ShadowVm},
        Call, ExecutionResult, Halt, InspectExecutionMode, OneshotEnv, OneshotTracingParams,
        OneshotTransactionExecutionResult, StoredL2BlockEnv, TxExecutionArgs, TxExecutionMode,
        VmFactory, VmInterface,
    },
    is_supported_by_fast_vm,
    tracers::{CallTracer, StorageInvocations, TracerDispatcher, ValidationTracer},
    utils::adjust_pubdata_price_for_tx,
    vm_fast::{self, FastValidationTracer, StorageInvocationsTracer},
    vm_latest::{HistoryDisabled, HistoryEnabled},
    zk_evm_latest::ethereum_types::U256,
    FastVmInstance, HistoryMode, LegacyVmInstance, MultiVmTracer, VmVersion,
};
use zksync_types::{
    block::pack_block_info,
    get_nonce_key, h256_to_u256,
    l2::L2Tx,
    u256_to_h256,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    vm::FastVmMode,
    AccountTreeId, Nonce, StopGuard, StopToken, StorageKey, Transaction, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
};

pub use self::{
    block::{BlockInfo, ResolvedBlockInfo},
    contracts::{
        BaseSystemContractsProvider, CallOrExecute, ContractsKind, EstimateGas,
        MultiVmBaseSystemContracts,
    },
    env::OneshotEnvParameters,
    mock::MockOneshotExecutor,
};
use crate::shared::RuntimeContextStorageMetrics;

mod block;
mod contracts;
mod env;
mod metrics;
mod mock;
#[cfg(test)]
mod tests;

/// Main [`OneshotExecutor`] implementation used by the API server.
#[derive(Debug)]
pub struct MainOneshotExecutor {
    fast_vm_mode: FastVmMode,
    vm_divergence_handler: DivergenceHandler,
    missed_storage_invocation_limit: usize,
    execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
    interrupted_execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
}

impl MainOneshotExecutor {
    /// Creates a new executor with the specified limit of cache misses for storage read operations (an anti-DoS measure).
    /// The limit is applied for calls and gas estimations, but not during transaction validation.
    pub fn new(missed_storage_invocation_limit: usize) -> Self {
        Self {
            fast_vm_mode: FastVmMode::Old,
            vm_divergence_handler: DivergenceHandler::new(|_, _| {
                // Do nothing
            }),
            missed_storage_invocation_limit,
            execution_latency_histogram: None,
            interrupted_execution_latency_histogram: None,
        }
    }

    /// Sets the fast VM mode used by this executor.
    pub fn set_fast_vm_mode(&mut self, fast_vm_mode: FastVmMode) {
        if !matches!(fast_vm_mode, FastVmMode::Old) {
            tracing::warn!(
                "Running new VM with modes {fast_vm_mode:?}; this can lead to incorrect node behavior"
            );
        }
        self.fast_vm_mode = fast_vm_mode;
    }

    /// Sets the handler called when a VM divergence is detected. Regardless of the handler, the divergence error(s)
    /// will be logged on the `ERROR` level.
    pub fn set_divergence_handler(&mut self, handler: DivergenceHandler) {
        self.vm_divergence_handler = handler;
    }

    /// Sets a histogram for measuring VM execution latency.
    pub fn set_execution_latency_histogram(
        &mut self,
        histogram: &'static vise::Histogram<Duration>,
    ) {
        self.execution_latency_histogram = Some(histogram);
    }

    pub fn set_interrupted_execution_latency_histogram(
        &mut self,
        histogram: &'static vise::Histogram<Duration>,
    ) {
        self.interrupted_execution_latency_histogram = Some(histogram);
    }

    fn select_fast_vm_mode(
        &self,
        env: &OneshotEnv,
        tracing_params: &OneshotTracingParams,
    ) -> FastVmMode {
        if tracing_params.trace_calls || !is_supported_by_fast_vm(env.system.version) {
            FastVmMode::Old // the fast VM doesn't support call tracing or old protocol versions
        } else {
            self.fast_vm_mode
        }
    }
}

#[async_trait]
impl<S> OneshotExecutor<StorageWithOverrides<S>> for MainOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: StorageWithOverrides<S>,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult> {
        let missed_storage_invocation_limit = match env.system.execution_mode {
            // storage accesses are not limited for tx validation
            TxExecutionMode::VerifyExecute => usize::MAX,
            TxExecutionMode::EthCall | TxExecutionMode::EstimateFee => {
                self.missed_storage_invocation_limit
            }
        };
        let op_name = match env.system.execution_mode {
            TxExecutionMode::VerifyExecute => "oneshot_vm#execute",
            TxExecutionMode::EthCall => "oneshot_vm#call",
            TxExecutionMode::EstimateFee => "oneshot_vm#estimate_fee",
        };

        let (_stop_guard, stop_token) = StopGuard::new();
        let sandbox = VmSandbox {
            fast_vm_mode: self.select_fast_vm_mode(&env, &tracing_params),
            vm_divergence_handler: self.vm_divergence_handler.clone(),
            storage,
            env,
            stop_token,
            execution_args: args,
            execution_latency_histogram: self.execution_latency_histogram,
            interrupted_execution_latency_histogram: self.interrupted_execution_latency_histogram,
        };

        let current_span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _entered_span = current_span.entered();
            let _guard = AllocationGuard::new(op_name);
            sandbox.execute_in_vm(|stop_token, vm, transaction| {
                vm.inspect_transaction_with_bytecode_compression(
                    stop_token.clone(),
                    missed_storage_invocation_limit,
                    tracing_params,
                    transaction,
                    true,
                )
            })
        })
        .await
        .context("VM execution panicked")
    }
}

#[async_trait]
impl<S> TransactionValidator<StorageWithOverrides<S>> for MainOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn validate_transaction(
        &self,
        storage: StorageWithOverrides<S>,
        env: OneshotEnv,
        tx: L2Tx,
        validation_params: ValidationParams,
    ) -> anyhow::Result<Result<ValidationTraces, ValidationError>> {
        anyhow::ensure!(
            env.system.execution_mode == TxExecutionMode::VerifyExecute,
            "Unexpected execution mode for tx validation: {:?} (expected `VerifyExecute`)",
            env.system.execution_mode
        );

        let l1_batch_env = env.l1_batch.clone();
        let (_stop_guard, stop_token) = StopGuard::new();
        let sandbox = VmSandbox {
            fast_vm_mode: if !is_supported_by_fast_vm(env.system.version) {
                FastVmMode::Old // the fast VM doesn't support old protocol versions
            } else {
                self.fast_vm_mode
            },
            vm_divergence_handler: self.vm_divergence_handler.clone(),
            storage,
            env,
            stop_token,
            execution_args: TxExecutionArgs::for_validation(tx),
            execution_latency_histogram: self.execution_latency_histogram,
            interrupted_execution_latency_histogram: self.interrupted_execution_latency_histogram,
        };

        let current_span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _entered_span = current_span.entered();
            let _guard = AllocationGuard::new("oneshot#validate");
            let version = sandbox.env.system.version.into();
            let batch_timestamp = l1_batch_env.timestamp;

            sandbox.execute_in_vm(|_, vm, transaction| match vm {
                Vm::Legacy(vm) => {
                    vm.push_transaction(transaction);
                    validate_legacy(vm, version, validation_params, batch_timestamp)
                }

                Vm::Fast(_, FastVmInstance::Fast(vm)) => {
                    vm.push_transaction(transaction);
                    validate_fast(vm, validation_params, batch_timestamp)
                }

                Vm::Fast(_, FastVmInstance::Shadowed(vm)) => {
                    vm.push_transaction(transaction);
                    vm.get_custom_mut("validation result", |vm| match vm {
                        ShadowMut::Main(vm) => validate_legacy::<_, HistoryEnabled>(
                            vm,
                            version,
                            validation_params.clone(),
                            batch_timestamp,
                        ),
                        ShadowMut::Shadow(vm) => {
                            validate_fast(vm, validation_params.clone(), batch_timestamp)
                        }
                    })
                }
            })
        })
        .await
        .context("VM execution panicked")
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum Vm<S: ReadStorage, Tr, Val> {
    Legacy(LegacyVmInstance<S, HistoryDisabled>),
    Fast(StoragePtr<StorageView<S>>, FastVmInstance<S, Tr, Val>),
}

impl<S: ReadStorage> Vm<S, StorageInvocationsTracer<StorageView<S>>, FastValidationTracer> {
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        stop_token: StopToken,
        missed_storage_invocation_limit: usize,
        params: OneshotTracingParams,
        tx: Transaction,
        with_compression: bool,
    ) -> OneshotTransactionExecutionResult {
        let mut calls_result = Arc::<OnceCell<_>>::default();
        let (compression_result, tx_result) = match self {
            Self::Legacy(vm) => {
                let mut tracers = Self::create_legacy_tracers(
                    stop_token,
                    missed_storage_invocation_limit,
                    params.trace_calls.then(|| calls_result.clone()),
                );
                vm.inspect_transaction_with_bytecode_compression(&mut tracers, tx, with_compression)
            }
            Self::Fast(storage, vm) => {
                assert!(
                    !params.trace_calls,
                    "Call tracing is not supported by fast VM yet"
                );
                let legacy_tracers = Self::create_legacy_tracers::<HistoryEnabled>(
                    stop_token.clone(),
                    missed_storage_invocation_limit,
                    None,
                );
                let tracer =
                    StorageInvocationsTracer::new(storage.clone(), missed_storage_invocation_limit)
                        .with_stop_token(stop_token);
                let mut full_tracer = (
                    legacy_tracers.into(),
                    (tracer, FastValidationTracer::default()),
                );
                let mut result = vm.inspect_transaction_with_bytecode_compression(
                    &mut full_tracer,
                    tx,
                    with_compression,
                );

                if let ExecutionResult::Halt {
                    reason: Halt::TracerCustom(msg),
                } = &mut result.1.result
                {
                    // Patch the halt message to be more specific; the fast VM provides a generic one since it doesn't know
                    // which tracer(s) are run. Here, we do know that the only tracer capable of stopping VM execution is the storage limiter.
                    *msg = "Storage invocations limit reached".to_owned();
                }

                result
            }
        };

        OneshotTransactionExecutionResult {
            tx_result: Box::new(tx_result),
            compression_result: compression_result.map(drop),
            call_traces: Arc::make_mut(&mut calls_result).take().unwrap_or_default(),
        }
    }

    fn create_legacy_tracers<H: HistoryMode>(
        stop_token: StopToken,
        missed_storage_invocation_limit: usize,
        calls_result: Option<Arc<OnceCell<Vec<Call>>>>,
    ) -> TracerDispatcher<StorageView<S>, H> {
        let mut tracers = vec![];
        if let Some(calls_result) = calls_result {
            tracers.push(CallTracer::new(calls_result).into_tracer_pointer());
        }
        let storage_limiter =
            StorageInvocations::new(missed_storage_invocation_limit).with_stop_token(stop_token);
        tracers.push(storage_limiter.into_tracer_pointer());
        tracers.into()
    }
}

fn validate_fast<S: ReadStorage>(
    vm: &mut vm_fast::Vm<S, (), vm_fast::FullValidationTracer>,
    validation_params: ValidationParams,
    batch_timestamp: u64,
) -> Result<ValidationTraces, ValidationError> {
    let validation = vm_fast::FullValidationTracer::new(validation_params, batch_timestamp);
    let mut tracer = ((), validation);
    let result_and_logs = vm.inspect(&mut tracer, InspectExecutionMode::OneTx);
    if let Some(violation) = tracer.1.validation_error() {
        return Err(ValidationError::ViolatedRule(violation));
    }

    match result_and_logs.result {
        ExecutionResult::Halt { reason } => Err(ValidationError::FailedTx(reason)),
        ExecutionResult::Revert { .. } => {
            unreachable!("Revert can only happen at the end of a transaction")
        }
        ExecutionResult::Success { .. } => Ok(tracer.1.traces()),
    }
}

fn validate_legacy<S, H>(
    vm: &mut impl VmInterface<TracerDispatcher: From<TracerDispatcher<S, H>>>,
    version: VmVersion,
    validation_params: ValidationParams,
    batch_timestamp: u64,
) -> Result<ValidationTraces, ValidationError>
where
    S: WriteStorage,
    H: 'static + HistoryMode,
    ValidationTracer<H>: MultiVmTracer<S, H>,
{
    let validation_tracer = ValidationTracer::<H>::new(validation_params, version, batch_timestamp);
    let mut validation_result = validation_tracer.get_result();
    let validation_traces = validation_tracer.get_traces();
    let validation_tracer: Box<dyn MultiVmTracer<_, H>> = validation_tracer.into_tracer_pointer();
    let tracers = TracerDispatcher::from(validation_tracer);

    let exec_result = vm.inspect(&mut tracers.into(), InspectExecutionMode::OneTx);

    let validation_result = Arc::make_mut(&mut validation_result)
        .take()
        .map_or(Ok(()), Err);

    match (exec_result.result, validation_result) {
        (_, Err(violated_rule)) => Err(ValidationError::ViolatedRule(violated_rule)),
        (ExecutionResult::Halt { reason }, _) => Err(ValidationError::FailedTx(reason)),
        _ => Ok(validation_traces.lock().unwrap().clone()),
    }
}

/// Full parameters necessary to instantiate a VM for oneshot execution.
#[derive(Debug)]
struct VmSandbox<S> {
    fast_vm_mode: FastVmMode,
    vm_divergence_handler: DivergenceHandler,
    storage: StorageWithOverrides<S>,
    env: OneshotEnv,
    stop_token: StopToken,
    execution_args: TxExecutionArgs,
    execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
    interrupted_execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
}

impl<S: ReadStorage> VmSandbox<S> {
    /// This method is blocking.
    fn setup_storage(
        storage: &mut StorageWithOverrides<S>,
        execution_args: &TxExecutionArgs,
        current_block: Option<StoredL2BlockEnv>,
    ) {
        let storage_view_setup_started_at = Instant::now();
        if let Some(nonce) = execution_args.enforced_nonce {
            let nonce_key = get_nonce_key(&execution_args.transaction.initiator_account());
            let full_nonce = storage.read_value(&nonce_key);
            let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
            let enforced_full_nonce = nonces_to_full_nonce(U256::from(nonce.0), deployment_nonce);
            storage.set_value(nonce_key, u256_to_h256(enforced_full_nonce));
        }

        let payer = execution_args.transaction.payer();
        let balance_key = storage_key_for_eth_balance(&payer);
        let mut current_balance = h256_to_u256(storage.read_value(&balance_key));
        current_balance += execution_args.added_balance;
        storage.set_value(balance_key, u256_to_h256(current_balance));

        // Reset L2 block info if necessary.
        if let Some(current_block) = current_block {
            let l2_block_info_key = StorageKey::new(
                AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
                SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
            );
            let l2_block_info =
                pack_block_info(current_block.number.into(), current_block.timestamp);
            storage.set_value(l2_block_info_key, u256_to_h256(l2_block_info));

            let l2_block_txs_rolling_hash_key = StorageKey::new(
                AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
                SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
            );
            storage.set_value(
                l2_block_txs_rolling_hash_key,
                current_block.txs_rolling_hash,
            );
        }

        let storage_view_setup_time = storage_view_setup_started_at.elapsed();
        // We don't want to emit too many logs.
        if storage_view_setup_time > Duration::from_millis(10) {
            tracing::debug!("Prepared the storage view (took {storage_view_setup_time:?})",);
        }
    }

    fn execute_in_vm<T, Tr, Val>(
        mut self,
        action: impl FnOnce(&StopToken, &mut Vm<StorageWithOverrides<S>, Tr, Val>, Transaction) -> T,
    ) -> T
    where
        Tr: vm_fast::interface::Tracer + Default,
        Val: vm_fast::ValidationTracer,
    {
        Self::setup_storage(
            &mut self.storage,
            &self.execution_args,
            self.env.current_block,
        );

        let protocol_version = self.env.system.version;
        let mode = self.env.system.execution_mode;
        if self.execution_args.adjust_pubdata_price {
            self.env.l1_batch.fee_input = adjust_pubdata_price_for_tx(
                self.env.l1_batch.fee_input,
                self.execution_args.transaction.gas_per_pubdata_byte_limit(),
                self.env.l1_batch.enforced_base_fee.map(U256::from),
                protocol_version.into(),
            );
        };

        let transaction = self.execution_args.transaction;
        let tx_id = format!(
            "{:?}-{}",
            transaction.initiator_account(),
            transaction.nonce().unwrap_or(Nonce(0))
        );

        let storage_view = StorageView::new(self.storage).to_rc_ptr();
        let mut vm = match self.fast_vm_mode {
            FastVmMode::Old => Vm::Legacy(LegacyVmInstance::new_with_specific_version(
                self.env.l1_batch,
                self.env.system,
                storage_view.clone(),
                protocol_version.into_api_vm_version(),
            )),
            FastVmMode::New => Vm::Fast(
                storage_view.clone(),
                FastVmInstance::fast(self.env.l1_batch, self.env.system, storage_view.clone()),
            ),
            FastVmMode::Shadow => {
                let mut vm =
                    ShadowVm::new(self.env.l1_batch, self.env.system, storage_view.clone());
                let transaction = format!("{transaction:?}");
                let full_handler = DivergenceHandler::new(move |errors, vm_dump| {
                    tracing::error!(transaction, ?mode, "{errors}");
                    self.vm_divergence_handler.handle(errors, vm_dump);
                });
                vm.set_divergence_handler(full_handler);
                Vm::Fast(storage_view.clone(), FastVmInstance::Shadowed(vm))
            }
        };

        let started_at = Instant::now();
        let result = action(&self.stop_token, &mut vm, transaction);
        let vm_execution_took = started_at.elapsed();
        let was_interrupted = self.stop_token.should_stop();

        if let Some(histogram) = self.execution_latency_histogram {
            histogram.observe(vm_execution_took);
        }
        if let (true, Some(histogram)) = (
            was_interrupted,
            self.interrupted_execution_latency_histogram,
        ) {
            histogram.observe(vm_execution_took);
        }

        match &vm {
            Vm::Legacy(vm) => {
                let memory_metrics = vm.record_vm_memory_metrics();
                let stats = storage_view.borrow().stats();
                metrics::report_vm_memory_metrics(&memory_metrics, &stats);
                RuntimeContextStorageMetrics::observe(
                    &format!("Tx {tx_id}"),
                    was_interrupted,
                    vm_execution_took,
                    &stats,
                );
            }
            Vm::Fast(..) => {
                // The new VM implementation doesn't have the same memory model as old ones, so it doesn't report memory metrics,
                // only storage-related ones.
                RuntimeContextStorageMetrics::observe(
                    &format!("Tx {tx_id}"),
                    was_interrupted,
                    vm_execution_took,
                    &storage_view.borrow().stats(),
                );
            }
        }
        result
    }
}
