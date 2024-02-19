use std::fmt;

use multivm::interface::{ExecutionResult, VmExecutionResultAndLogs};
use zksync_types::{
    fee::TransactionExecutionMetrics, l2::L2Tx, ExecuteTransactionCommon, Transaction,
};

use super::{
    execute::{TransactionExecutionOutput, TransactionExecutor},
    validate::ValidationError,
    BlockArgs,
};

type TxResponseFn = dyn Fn(&Transaction, &BlockArgs) -> ExecutionResult + Send + Sync;

pub(crate) struct MockTransactionExecutor {
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
    pub fn set_call_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &BlockArgs) -> ExecutionResult + 'static + Send + Sync,
    {
        self.call_responses = Box::new(responses);
    }

    pub fn set_tx_responses<F>(&mut self, responses: F)
    where
        F: Fn(&Transaction, &BlockArgs) -> ExecutionResult + 'static + Send + Sync,
    {
        self.tx_responses = Box::new(responses);
    }

    pub fn validate_tx(&self, tx: L2Tx, block_args: &BlockArgs) -> Result<(), ValidationError> {
        let result = (self.tx_responses)(&tx.into(), block_args);
        match result {
            ExecutionResult::Success { .. } => Ok(()),
            other => Err(ValidationError::Internal(anyhow::anyhow!(
                "transaction validation failed: {other:?}"
            ))),
        }
    }

    pub fn execute_tx(
        &self,
        tx: &Transaction,
        block_args: &BlockArgs,
    ) -> anyhow::Result<TransactionExecutionOutput> {
        let result = self.get_execution_result(tx, block_args);
        let output = TransactionExecutionOutput {
            vm: VmExecutionResultAndLogs {
                result,
                logs: Default::default(),
                statistics: Default::default(),
                refunds: Default::default(),
            },
            metrics: TransactionExecutionMetrics::default(),
            are_published_bytecodes_ok: true,
        };
        Ok(output)
    }

    fn get_execution_result(&self, tx: &Transaction, block_args: &BlockArgs) -> ExecutionResult {
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
