use std::collections::HashMap;

use multivm::{
    interface::{ExecutionResult, VmExecutionResultAndLogs},
    tracers::validator::ValidationError,
};
use zksync_types::{
    fee::TransactionExecutionMetrics, l2::L2Tx, ExecuteTransactionCommon, Transaction, H256,
};

use super::TransactionExecutor;

type MockExecutionOutput = (VmExecutionResultAndLogs, TransactionExecutionMetrics, bool);

#[derive(Debug, Default)]
pub(crate) struct MockTransactionExecutor {
    call_responses: HashMap<Vec<u8>, MockExecutionOutput>,
    tx_responses: HashMap<H256, MockExecutionOutput>,
}

impl MockTransactionExecutor {
    pub fn insert_call_response(&mut self, calldata: Vec<u8>, result: ExecutionResult) {
        let result = VmExecutionResultAndLogs {
            result,
            logs: Default::default(),
            statistics: Default::default(),
            refunds: Default::default(),
        };
        let output = (result, TransactionExecutionMetrics::default(), true);
        self.call_responses.insert(calldata, output);
    }

    pub fn insert_tx_response(&mut self, tx_hash: H256, result: ExecutionResult) {
        let result = VmExecutionResultAndLogs {
            result,
            logs: Default::default(),
            statistics: Default::default(),
            refunds: Default::default(),
        };
        let output = (result, TransactionExecutionMetrics::default(), true);
        self.tx_responses.insert(tx_hash, output);
    }

    pub fn validate_tx(&self, tx: &L2Tx) -> Result<(), ValidationError> {
        self.tx_responses
            .get(&tx.hash())
            .unwrap_or_else(|| panic!("Validating unexpected transaction: {tx:?}"));
        Ok(())
    }

    pub fn execute_tx(&self, tx: &Transaction) -> MockExecutionOutput {
        if let ExecuteTransactionCommon::L2(data) = &tx.common_data {
            if data.input.is_none() {
                // `Transaction` was obtained from a `CallRequest`
                return self
                    .call_responses
                    .get(tx.execute.calldata())
                    .unwrap_or_else(|| panic!("Executing unexpected call: {tx:?}"))
                    .clone();
            }
        }

        self.tx_responses
            .get(&tx.hash())
            .unwrap_or_else(|| panic!("Executing unexpected transaction: {tx:?}"))
            .clone()
    }
}

impl From<MockTransactionExecutor> for TransactionExecutor {
    fn from(executor: MockTransactionExecutor) -> Self {
        Self::Mock(executor)
    }
}
