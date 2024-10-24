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
use zksync_multivm::{
    interface::{
        executor::{OneshotExecutor, TransactionValidator},
        storage::{ReadStorage, StorageView, StorageWithOverrides},
        tracer::{ValidationError, ValidationParams},
        utils::{DivergenceHandler, ShadowVm},
        Call, ExecutionResult, InspectExecutionMode, OneshotEnv, OneshotTracingParams,
        OneshotTransactionExecutionResult, StoredL2BlockEnv, TxExecutionArgs, TxExecutionMode,
        VmFactory, VmInterface,
    },
    is_supported_by_fast_vm,
    tracers::{CallTracer, StorageInvocations, TracerDispatcher, ValidationTracer},
    utils::adjust_pubdata_price_for_tx,
    vm_latest::{HistoryDisabled, HistoryEnabled},
    zk_evm_latest::ethereum_types::U256,
    FastVmInstance, HistoryMode, LegacyVmInstance, MultiVMTracer,
};
use zksync_types::{
    block::pack_block_info,
    get_nonce_key,
    l2::L2Tx,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    vm::FastVmMode,
    AccountTreeId, Nonce, StorageKey, Transaction, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

pub use self::{
    block::{BlockInfo, ResolvedBlockInfo},
    contracts::{
        BaseSystemContractsProvider, CallOrExecute, ContractsKind, EstimateGas,
        MultiVMBaseSystemContracts,
    },
    env::OneshotEnvParameters,
    mock::MockOneshotExecutor,
};

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
    panic_on_divergence: bool,
    missed_storage_invocation_limit: usize,
    execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
}

impl MainOneshotExecutor {
    /// Creates a new executor with the specified limit of cache misses for storage read operations (an anti-DoS measure).
    /// The limit is applied for calls and gas estimations, but not during transaction validation.
    pub fn new(missed_storage_invocation_limit: usize) -> Self {
        Self {
            fast_vm_mode: FastVmMode::Old,
            panic_on_divergence: false,
            missed_storage_invocation_limit,
            execution_latency_histogram: None,
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

    /// Causes the VM to panic on divergence whenever it executes in the shadow mode. By default, a divergence is logged on `ERROR` level.
    pub fn panic_on_divergence(&mut self) {
        self.panic_on_divergence = true;
    }

    /// Sets a histogram for measuring VM execution latency.
    pub fn set_execution_latency_histogram(
        &mut self,
        histogram: &'static vise::Histogram<Duration>,
    ) {
        self.execution_latency_histogram = Some(histogram);
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
        let sandbox = VmSandbox {
            fast_vm_mode: self.select_fast_vm_mode(&env, &tracing_params),
            panic_on_divergence: self.panic_on_divergence,
            storage,
            env,
            execution_args: args,
            execution_latency_histogram: self.execution_latency_histogram,
        };

        tokio::task::spawn_blocking(move || {
            sandbox.execute_in_vm(|vm, transaction| {
                vm.inspect_transaction_with_bytecode_compression(
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
    ) -> anyhow::Result<Result<(), ValidationError>> {
        anyhow::ensure!(
            env.system.execution_mode == TxExecutionMode::VerifyExecute,
            "Unexpected execution mode for tx validation: {:?} (expected `VerifyExecute`)",
            env.system.execution_mode
        );

        let sandbox = VmSandbox {
            fast_vm_mode: FastVmMode::Old,
            panic_on_divergence: self.panic_on_divergence,
            storage,
            env,
            execution_args: TxExecutionArgs::for_validation(tx),
            execution_latency_histogram: self.execution_latency_histogram,
        };

        tokio::task::spawn_blocking(move || {
            let (validation_tracer, mut validation_result) =
                ValidationTracer::<HistoryDisabled>::new(
                    validation_params,
                    sandbox.env.system.version.into(),
                );
            let tracers = vec![validation_tracer.into_tracer_pointer()];

            let exec_result = sandbox.execute_in_vm(|vm, transaction| {
                let Vm::Legacy(vm) = vm else {
                    unreachable!("Fast VM is never used for validation yet");
                };
                vm.push_transaction(transaction);
                vm.inspect(&mut tracers.into(), InspectExecutionMode::OneTx)
            });
            let validation_result = Arc::make_mut(&mut validation_result)
                .take()
                .map_or(Ok(()), Err);

            match (exec_result.result, validation_result) {
                (_, Err(violated_rule)) => Err(ValidationError::ViolatedRule(violated_rule)),
                (ExecutionResult::Halt { reason }, _) => Err(ValidationError::FailedTx(reason)),
                _ => Ok(()),
            }
        })
        .await
        .context("VM execution panicked")
    }
}

#[derive(Debug)]
enum Vm<S: ReadStorage> {
    Legacy(LegacyVmInstance<S, HistoryDisabled>),
    Fast(FastVmInstance<S, ()>),
}

impl<S: ReadStorage> Vm<S> {
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        missed_storage_invocation_limit: usize,
        params: OneshotTracingParams,
        tx: Transaction,
        with_compression: bool,
    ) -> OneshotTransactionExecutionResult {
        let mut calls_result = Arc::<OnceCell<_>>::default();
        let (compression_result, tx_result) = match self {
            Self::Legacy(vm) => {
                let mut tracers = Self::create_legacy_tracers(
                    missed_storage_invocation_limit,
                    params.trace_calls.then(|| calls_result.clone()),
                );
                vm.inspect_transaction_with_bytecode_compression(&mut tracers, tx, with_compression)
            }
            Self::Fast(vm) => {
                assert!(
                    !params.trace_calls,
                    "Call tracing is not supported by fast VM yet"
                );
                let legacy_tracers = Self::create_legacy_tracers::<HistoryEnabled>(
                    missed_storage_invocation_limit,
                    None,
                );
                let mut full_tracer = (legacy_tracers.into(), ());
                vm.inspect_transaction_with_bytecode_compression(
                    &mut full_tracer,
                    tx,
                    with_compression,
                )
            }
        };

        OneshotTransactionExecutionResult {
            tx_result: Box::new(tx_result),
            compression_result: compression_result.map(drop),
            call_traces: Arc::make_mut(&mut calls_result).take().unwrap_or_default(),
        }
    }

    fn create_legacy_tracers<H: HistoryMode>(
        missed_storage_invocation_limit: usize,
        calls_result: Option<Arc<OnceCell<Vec<Call>>>>,
    ) -> TracerDispatcher<StorageView<S>, H> {
        let mut tracers = vec![];
        if let Some(calls_result) = calls_result {
            tracers.push(CallTracer::new(calls_result).into_tracer_pointer());
        }
        tracers
            .push(StorageInvocations::new(missed_storage_invocation_limit).into_tracer_pointer());
        tracers.into()
    }
}

/// Full parameters necessary to instantiate a VM for oneshot execution.
#[derive(Debug)]
struct VmSandbox<S> {
    fast_vm_mode: FastVmMode,
    panic_on_divergence: bool,
    storage: StorageWithOverrides<S>,
    env: OneshotEnv,
    execution_args: TxExecutionArgs,
    execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
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

    /// This method is blocking.
    fn execute_in_vm<T>(
        mut self,
        action: impl FnOnce(&mut Vm<StorageWithOverrides<S>>, Transaction) -> T,
    ) -> T {
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
            FastVmMode::New => Vm::Fast(FastVmInstance::fast(
                self.env.l1_batch,
                self.env.system,
                storage_view.clone(),
            )),
            FastVmMode::Shadow => {
                let mut vm =
                    ShadowVm::new(self.env.l1_batch, self.env.system, storage_view.clone());
                if !self.panic_on_divergence {
                    let transaction = format!("{:?}", transaction);
                    let handler = DivergenceHandler::new(move |errors, _| {
                        tracing::error!(transaction, ?mode, "{errors}");
                    });
                    vm.set_divergence_handler(handler);
                }
                Vm::Fast(FastVmInstance::Shadowed(vm))
            }
        };

        let started_at = Instant::now();
        let result = action(&mut vm, transaction);
        let vm_execution_took = started_at.elapsed();

        if let Some(histogram) = self.execution_latency_histogram {
            histogram.observe(vm_execution_took);
        }

        match &vm {
            Vm::Legacy(vm) => {
                let memory_metrics = vm.record_vm_memory_metrics();
                metrics::report_vm_memory_metrics(
                    &tx_id,
                    &memory_metrics,
                    vm_execution_took,
                    &storage_view.borrow().stats(),
                );
            }
            Vm::Fast(_) => {
                // The new VM implementation doesn't have the same memory model as old ones, so it doesn't report memory metrics,
                // only storage-related ones.
                metrics::report_vm_storage_metrics(
                    &format!("Tx {tx_id}"),
                    vm_execution_took,
                    &storage_view.borrow().stats(),
                );
            }
        }
        result
    }
}
