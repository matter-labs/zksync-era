use std::fmt;

use zksync_multivm::interface::{ExecutionResult, VmExecutionResultAndLogs};
use zksync_types::{
    fee::TransactionExecutionMetrics, ExecuteTransactionCommon, ExternalTx, Transaction,
};

use super::{
    execute::{TransactionExecutionOutput, TransactionExecutor},
    validate::ValidationError,
    BlockArgs,
};

type TxResponseFn = dyn Fn(&Transaction, &BlockArgs) -> VmExecutionResultAndLogs + Send + Sync;

pub struct MockTransactionExecutor {
    call_responses: Box<TxResponseFn>,
    tx_responses: Box<TxResponseFn>,
}

impl fmt::Debug for MockTransactionExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockTransactionExecutor")
            .finish_non_exhaustive()
    }
}

impl Default for MockTransactionExecutor {
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

impl MockTransactionExecutor {
    #[cfg(test)]
    pub(crate) fn set_call_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &BlockArgs) -> ExecutionResult + 'static + Send + Sync,
    {
        self.call_responses = self.wrap_responses(responses);
    }

    #[cfg(test)]
    pub(crate) fn set_tx_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &BlockArgs) -> ExecutionResult + 'static + Send + Sync,
    {
        self.tx_responses = self.wrap_responses(responses);
    }

    #[cfg(test)]
    fn wrap_responses<F>(&mut self, responses: F) -> Box<TxResponseFn>
    where
        F: Fn(&Transaction, &BlockArgs) -> ExecutionResult + 'static + Send + Sync,
    {
        Box::new(
            move |tx: &Transaction, ba: &BlockArgs| -> VmExecutionResultAndLogs {
                VmExecutionResultAndLogs {
                    result: responses(tx, ba),
                    logs: Default::default(),
                    statistics: Default::default(),
                    refunds: Default::default(),
                }
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn set_tx_responses_with_logs<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &BlockArgs) -> VmExecutionResultAndLogs + 'static + Send + Sync,
    {
        self.tx_responses = Box::new(responses);
    }

    pub(crate) fn validate_tx(
        &self,
        tx: ExternalTx,
        block_args: &BlockArgs,
    ) -> Result<(), ValidationError> {
        let result = (self.tx_responses)(&tx.into(), block_args);
        match result.result {
            ExecutionResult::Success { .. } => Ok(()),
            other => Err(ValidationError::Internal(anyhow::anyhow!(
                "transaction validation failed: {other:?}"
            ))),
        }
    }

    pub(crate) fn execute_tx(
        &self,
        tx: &Transaction,
        block_args: &BlockArgs,
    ) -> anyhow::Result<TransactionExecutionOutput> {
        let result = self.get_execution_result(tx, block_args);
        let output = TransactionExecutionOutput {
            vm: result,
            metrics: TransactionExecutionMetrics::default(),
            are_published_bytecodes_ok: true,
        };

        Ok(output)
    }

    fn get_execution_result(
        &self,
        tx: &Transaction,
        block_args: &BlockArgs,
    ) -> VmExecutionResultAndLogs {
        if let ExecuteTransactionCommon::L2(data) = &tx.common_data {
            if data.input.is_none() {
                return (self.call_responses)(tx, block_args);
            }
        }
        (self.tx_responses)(tx, block_args)
    }
}

impl From<MockTransactionExecutor> for TransactionExecutor {
    fn from(executor: MockTransactionExecutor) -> Self {
        Self::Mock(executor)
    }
}
