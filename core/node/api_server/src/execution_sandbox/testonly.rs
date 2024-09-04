use std::fmt;

use async_trait::async_trait;
#[cfg(test)]
use zksync_multivm::interface::ExecutionResult;
use zksync_multivm::interface::{
    storage::ReadStorage, BytecodeCompressionError, OneshotEnv, TxExecutionMode,
    VmExecutionResultAndLogs,
};
use zksync_types::Transaction;

use super::{execute::TransactionExecutor, OneshotExecutor, TxExecutionArgs};

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
                panic!(
                    "Unexpected call with data {}",
                    hex::encode(tx.execute.calldata())
                );
            }),
            tx_responses: Box::new(|tx, _| {
                panic!("Unexpect transaction call: {tx:?}");
            }),
        }
    }
}

impl MockOneshotExecutor {
    #[cfg(test)]
    pub(crate) fn set_call_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        self.call_responses = self.wrap_responses(responses);
    }

    #[cfg(test)]
    pub(crate) fn set_tx_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &OneshotEnv) -> ExecutionResult + 'static + Send + Sync,
    {
        self.tx_responses = self.wrap_responses(responses);
    }

    #[cfg(test)]
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
                    new_known_factory_deps: Default::default(),
                }
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn set_tx_responses_with_logs<F>(&mut self, responses: F)
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
    type Tracers = ();

    async fn inspect_transaction(
        &self,
        _storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        (): Self::Tracers,
    ) -> anyhow::Result<VmExecutionResultAndLogs> {
        Ok(self.mock_inspect(env, args))
    }

    async fn inspect_transaction_with_bytecode_compression(
        &self,
        _storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        (): Self::Tracers,
    ) -> anyhow::Result<(
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    )> {
        Ok((Ok(()), self.mock_inspect(env, args)))
    }
}

impl From<MockOneshotExecutor> for TransactionExecutor {
    fn from(executor: MockOneshotExecutor) -> Self {
        Self::Mock(executor)
    }
}
