use std::{fmt, time::Duration};

use async_trait::async_trait;
use zksync_multivm::interface::{
    executor::{OneshotExecutor, TransactionValidator},
    storage::ReadStorage,
    tracer::{ValidationError, ValidationParams, ValidationTraces},
    ExecutionResult, OneshotEnv, OneshotTracingParams, OneshotTransactionExecutionResult,
    TxExecutionArgs, TxExecutionMode, VmExecutionResultAndLogs,
};
use zksync_types::{l2::L2Tx, Transaction};

type TxResponseFn = dyn Fn(&Transaction, &OneshotEnv) -> VmExecutionResultAndLogs + Send + Sync;
type TxValidationTracesResponseFn =
    dyn Fn(&Transaction, &OneshotEnv) -> ValidationTraces + Send + Sync;

/// Mock [`OneshotExecutor`] implementation.
pub struct MockOneshotExecutor {
    call_responses: Box<TxResponseFn>,
    tx_responses: Box<TxResponseFn>,
    tx_validation_traces_responses: Box<TxValidationTracesResponseFn>,
    vm_delay: Duration,
}

impl fmt::Debug for MockOneshotExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockTransactionExecutor")
            .field("vm_delay", &self.vm_delay)
            .finish_non_exhaustive()
    }
}

impl Default for MockOneshotExecutor {
    fn default() -> Self {
        Self {
            call_responses: Box::new(|tx, _| {
                panic!("Unexpected call with data {:?}", tx.execute.calldata());
            }),
            tx_responses: Box::new(|tx, _| {
                panic!("Unexpected transaction call: {tx:?}");
            }),
            tx_validation_traces_responses: Box::new(|_, _| ValidationTraces::default()),
            vm_delay: Duration::ZERO,
        }
    }
}

impl MockOneshotExecutor {
    /// Sets call response closure used by this executor.
    pub fn set_call_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        self.call_responses = self.wrap_responses(responses);
    }

    /// Sets transaction response closure used by this executor. The closure will be called both for transaction execution / validation,
    /// and for gas estimation.
    pub fn set_tx_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        self.tx_responses = self.wrap_responses(responses);
    }

    pub fn set_tx_validation_traces_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ValidationTraces + 'static + Send + Sync,
    {
        self.tx_validation_traces_responses = Box::new(responses);
    }

    fn wrap_responses<F>(&mut self, responses: F) -> Box<TxResponseFn>
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        Box::new(
            move |tx: &Transaction, env: &OneshotEnv| -> VmExecutionResultAndLogs {
                VmExecutionResultAndLogs::mock(responses(tx, env))
            },
        )
    }

    /// Same as [`Self::set_tx_responses()`], but allows to customize returned VM logs etc.
    pub fn set_full_tx_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> VmExecutionResultAndLogs + 'static + Send + Sync,
    {
        self.tx_responses = Box::new(responses);
    }

    /// Sets an artificial delay for each VM invocation.
    pub fn set_vm_delay(&mut self, delay: Duration) {
        self.vm_delay = delay;
    }

    async fn mock_inspect(
        &self,
        env: &OneshotEnv,
        args: TxExecutionArgs,
    ) -> VmExecutionResultAndLogs {
        tokio::time::sleep(self.vm_delay).await;

        match env.system.execution_mode {
            TxExecutionMode::EthCall => (self.call_responses)(&args.transaction, env),
            TxExecutionMode::VerifyExecute | TxExecutionMode::EstimateFee => {
                (self.tx_responses)(&args.transaction, env)
            }
        }
    }
}

#[async_trait]
impl<S> OneshotExecutor<S> for MockOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn inspect_transaction_with_bytecode_compression(
        &self,
        _storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        _params: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult> {
        Ok(OneshotTransactionExecutionResult {
            tx_result: Box::new(self.mock_inspect(&env, args).await),
            compression_result: Ok(()),
            call_traces: vec![],
        })
    }
}

#[async_trait]
impl<S> TransactionValidator<S> for MockOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn validate_transaction(
        &self,
        _storage: S,
        env: OneshotEnv,
        tx: L2Tx,
        _validation_params: ValidationParams,
    ) -> anyhow::Result<Result<ValidationTraces, ValidationError>> {
        Ok(
            match self
                .mock_inspect(&env, TxExecutionArgs::for_validation(tx.clone()))
                .await
                .result
            {
                ExecutionResult::Halt { reason } => Err(ValidationError::FailedTx(reason)),
                ExecutionResult::Success { .. } | ExecutionResult::Revert { .. } => {
                    Ok((self.tx_validation_traces_responses)(&tx.into(), &env))
                }
            },
        )
    }
}
