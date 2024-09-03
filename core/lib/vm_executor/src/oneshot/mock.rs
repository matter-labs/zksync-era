use std::fmt;

use async_trait::async_trait;
use zksync_multivm::interface::{
    executor::OneshotExecutor,
    storage::ReadStorage,
    tracer::{ValidationError, ValidationParams},
    ExecutionResult, OneshotEnv, OneshotTracingParams, OneshotTransactionExecutionResult,
    TxExecutionArgs, TxExecutionMode, VmExecutionResultAndLogs,
};
use zksync_types::Transaction;

type TxResponseFn = dyn Fn(&Transaction, &OneshotEnv) -> VmExecutionResultAndLogs + Send + Sync;

pub struct MockOneshotExecutor {
    call_responses: Box<TxResponseFn>,
    tx_responses: Box<TxResponseFn>,
}

impl fmt::Debug for MockOneshotExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockTransactionExecutor")
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
                panic!("Unexpect transaction call: {tx:?}");
            }),
        }
    }
}

impl MockOneshotExecutor {
    pub fn set_call_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        self.call_responses = self.wrap_responses(responses);
    }

    pub fn set_tx_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        self.tx_responses = self.wrap_responses(responses);
    }

    fn wrap_responses<F>(&mut self, responses: F) -> Box<TxResponseFn>
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        Box::new(
            move |tx: &Transaction, env: &OneshotEnv| -> VmExecutionResultAndLogs {
                VmExecutionResultAndLogs {
                    result: responses(tx, env),
                    logs: Default::default(),
                    statistics: Default::default(),
                    refunds: Default::default(),
                }
            },
        )
    }

    pub fn set_tx_responses_with_logs<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> VmExecutionResultAndLogs + 'static + Send + Sync,
    {
        self.tx_responses = Box::new(responses);
    }

    fn mock_inspect(&self, env: OneshotEnv, args: TxExecutionArgs) -> VmExecutionResultAndLogs {
        match env.system.execution_mode {
            TxExecutionMode::EthCall => (self.call_responses)(&args.transaction, &env),
            TxExecutionMode::VerifyExecute | TxExecutionMode::EstimateFee => {
                (self.tx_responses)(&args.transaction, &env)
            }
        }
    }
}

#[async_trait]
impl<S> OneshotExecutor<S> for MockOneshotExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn validate_transaction(
        &self,
        _storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        _validation_params: ValidationParams,
    ) -> anyhow::Result<Result<(), ValidationError>> {
        Ok(match self.mock_inspect(env, args).result {
            ExecutionResult::Halt { reason } => Err(ValidationError::FailedTx(reason)),
            ExecutionResult::Success { .. } | ExecutionResult::Revert { .. } => Ok(()),
        })
    }

    async fn inspect_transaction_with_bytecode_compression(
        &self,
        _storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        _params: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult> {
        Ok(OneshotTransactionExecutionResult {
            tx_result: Box::new(self.mock_inspect(env, args)),
            compression_result: Ok(()),
            call_traces: vec![],
        })
    }
}
