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
        storage::{ReadStorage, StoragePtr, StorageView, WriteStorage},
        tracer::{ValidationError, ValidationParams},
        ExecutionResult, OneshotEnv, OneshotTracingParams, OneshotTransactionExecutionResult,
        StoredL2BlockEnv, TxExecutionArgs, TxExecutionMode, VmExecutionMode, VmInterface,
    },
    tracers::{CallTracer, StorageInvocations, ValidationTracer},
    utils::adjust_pubdata_price_for_tx,
    vm_latest::HistoryDisabled,
    zk_evm_latest::ethereum_types::U256,
    LegacyVmInstance, MultiVMTracer,
};
use zksync_types::{
    block::pack_block_info,
    get_nonce_key,
    l2::L2Tx,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
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

/// Main [`OneshotExecutor`] implementation used by the API server.
#[derive(Debug, Default)]
pub struct MainOneshotExecutor {
    missed_storage_invocation_limit: usize,
    execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
}

impl MainOneshotExecutor {
    /// Creates a new executor with the specified limit of cache misses for storage read operations (an anti-DoS measure).
    /// The limit is applied for calls and gas estimations, but not during transaction validation.
    pub fn new(missed_storage_invocation_limit: usize) -> Self {
        Self {
            missed_storage_invocation_limit,
            execution_latency_histogram: None,
        }
    }

    /// Sets a histogram for measuring VM execution latency.
    pub fn set_execution_latency_histogram(
        &mut self,
        histogram: &'static vise::Histogram<Duration>,
    ) {
        self.execution_latency_histogram = Some(histogram);
    }
}

#[async_trait]
impl<S> OneshotExecutor<S> for MainOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        params: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult> {
        let missed_storage_invocation_limit = match env.system.execution_mode {
            // storage accesses are not limited for tx validation
            TxExecutionMode::VerifyExecute => usize::MAX,
            TxExecutionMode::EthCall | TxExecutionMode::EstimateFee => {
                self.missed_storage_invocation_limit
            }
        };
        let execution_latency_histogram = self.execution_latency_histogram;

        tokio::task::spawn_blocking(move || {
            let mut tracers = vec![];
            let mut calls_result = Arc::<OnceCell<_>>::default();
            if params.trace_calls {
                tracers.push(CallTracer::new(calls_result.clone()).into_tracer_pointer());
            }
            tracers.push(
                StorageInvocations::new(missed_storage_invocation_limit).into_tracer_pointer(),
            );

            let executor = VmSandbox::new(storage, env, args, execution_latency_histogram);
            let mut result = executor.apply(|vm, transaction| {
                let (compression_result, tx_result) = vm
                    .inspect_transaction_with_bytecode_compression(
                        &mut tracers.into(),
                        transaction,
                        true,
                    );
                OneshotTransactionExecutionResult {
                    tx_result: Box::new(tx_result),
                    compression_result: compression_result.map(drop),
                    call_traces: vec![],
                }
            });

            result.call_traces = Arc::make_mut(&mut calls_result).take().unwrap_or_default();
            result
        })
        .await
        .context("VM execution panicked")
    }
}

#[async_trait]
impl<S> TransactionValidator<S> for MainOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn validate_transaction(
        &self,
        storage: S,
        env: OneshotEnv,
        tx: L2Tx,
        validation_params: ValidationParams,
    ) -> anyhow::Result<Result<(), ValidationError>> {
        anyhow::ensure!(
            env.system.execution_mode == TxExecutionMode::VerifyExecute,
            "Unexpected execution mode for tx validation: {:?} (expected `VerifyExecute`)",
            env.system.execution_mode
        );
        let execution_latency_histogram = self.execution_latency_histogram;

        tokio::task::spawn_blocking(move || {
            let (validation_tracer, mut validation_result) =
                ValidationTracer::<HistoryDisabled>::new(
                    validation_params,
                    env.system.version.into(),
                );
            let tracers = vec![validation_tracer.into_tracer_pointer()];

            let executor = VmSandbox::new(
                storage,
                env,
                TxExecutionArgs::for_validation(tx),
                execution_latency_histogram,
            );
            let exec_result = executor.apply(|vm, transaction| {
                vm.push_transaction(transaction);
                vm.inspect(&mut tracers.into(), VmExecutionMode::OneTx)
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
struct VmSandbox<S: ReadStorage> {
    vm: Box<LegacyVmInstance<S, HistoryDisabled>>,
    storage_view: StoragePtr<StorageView<S>>,
    transaction: Transaction,
    execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
}

impl<S: ReadStorage> VmSandbox<S> {
    /// This method is blocking.
    fn new(
        storage: S,
        mut env: OneshotEnv,
        execution_args: TxExecutionArgs,
        execution_latency_histogram: Option<&'static vise::Histogram<Duration>>,
    ) -> Self {
        let mut storage_view = StorageView::new(storage);
        Self::setup_storage_view(&mut storage_view, &execution_args, env.current_block);

        let protocol_version = env.system.version;
        if execution_args.adjust_pubdata_price {
            env.l1_batch.fee_input = adjust_pubdata_price_for_tx(
                env.l1_batch.fee_input,
                execution_args.transaction.gas_per_pubdata_byte_limit(),
                env.l1_batch.enforced_base_fee.map(U256::from),
                protocol_version.into(),
            );
        };

        let storage_view = storage_view.to_rc_ptr();
        let vm = Box::new(LegacyVmInstance::new_with_specific_version(
            env.l1_batch,
            env.system,
            storage_view.clone(),
            protocol_version.into_api_vm_version(),
        ));

        Self {
            vm,
            storage_view,
            transaction: execution_args.transaction,
            execution_latency_histogram,
        }
    }

    /// This method is blocking.
    fn setup_storage_view(
        storage_view: &mut StorageView<S>,
        execution_args: &TxExecutionArgs,
        current_block: Option<StoredL2BlockEnv>,
    ) {
        let storage_view_setup_started_at = Instant::now();
        if let Some(nonce) = execution_args.enforced_nonce {
            let nonce_key = get_nonce_key(&execution_args.transaction.initiator_account());
            let full_nonce = storage_view.read_value(&nonce_key);
            let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
            let enforced_full_nonce = nonces_to_full_nonce(U256::from(nonce.0), deployment_nonce);
            storage_view.set_value(nonce_key, u256_to_h256(enforced_full_nonce));
        }

        let payer = execution_args.transaction.payer();
        let balance_key = storage_key_for_eth_balance(&payer);
        let mut current_balance = h256_to_u256(storage_view.read_value(&balance_key));
        current_balance += execution_args.added_balance;
        storage_view.set_value(balance_key, u256_to_h256(current_balance));

        // Reset L2 block info if necessary.
        if let Some(current_block) = current_block {
            let l2_block_info_key = StorageKey::new(
                AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
                SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
            );
            let l2_block_info =
                pack_block_info(current_block.number.into(), current_block.timestamp);
            storage_view.set_value(l2_block_info_key, u256_to_h256(l2_block_info));

            let l2_block_txs_rolling_hash_key = StorageKey::new(
                AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
                SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
            );
            storage_view.set_value(
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

    pub(super) fn apply<T, F>(mut self, apply_fn: F) -> T
    where
        F: FnOnce(&mut LegacyVmInstance<S, HistoryDisabled>, Transaction) -> T,
    {
        let tx_id = format!(
            "{:?}-{}",
            self.transaction.initiator_account(),
            self.transaction.nonce().unwrap_or(Nonce(0))
        );

        let started_at = Instant::now();
        let result = apply_fn(&mut *self.vm, self.transaction);
        let vm_execution_took = started_at.elapsed();

        if let Some(histogram) = self.execution_latency_histogram {
            histogram.observe(vm_execution_took);
        }
        let memory_metrics = self.vm.record_vm_memory_metrics();
        metrics::report_vm_memory_metrics(
            &tx_id,
            &memory_metrics,
            vm_execution_took,
            &self.storage_view.borrow().stats(),
        );
        result
    }
}
