use zksync_types::{ExecuteTransactionCommon, Transaction, H160, U256};

use super::VmTester;
use crate::{
    interface::{
        storage::ReadStorage, CurrentExecutionState, ExecutionResult, Halt, TxRevertReason,
        VmExecutionMode, VmExecutionResultAndLogs, VmInterface, VmInterfaceHistoryEnabled,
        VmRevertReason,
    },
    vm_fast::{vm::CircuitsTracer, Vm},
};

#[derive(Debug, Clone)]
pub(crate) enum TxModifier {
    WrongSignatureLength,
    WrongSignature,
    WrongMagicValue,
    WrongNonce,
    NonceReused,
}

#[derive(Debug, Clone)]
pub(crate) enum TxExpectedResult {
    Rejected { error: ExpectedError },
    Processed { rollback: bool },
}

#[derive(Debug, Clone)]
pub(crate) struct TransactionTestInfo {
    tx: Transaction,
    result: TxExpectedResult,
}

#[derive(Debug, Clone)]
pub(crate) struct ExpectedError {
    pub(crate) revert_reason: TxRevertReason,
    pub(crate) modifier: Option<TxModifier>,
}

impl From<TxModifier> for ExpectedError {
    fn from(value: TxModifier) -> Self {
        let revert_reason = match value {
            TxModifier::WrongSignatureLength => {
                Halt::ValidationFailed(VmRevertReason::General {
                    msg: "Signature length is incorrect".to_string(),
                    data: vec![
                        8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 83, 105, 103, 110, 97, 116, 117, 114, 101, 32,
                        108, 101, 110, 103, 116, 104, 32, 105, 115, 32, 105, 110, 99, 111, 114, 114, 101, 99,
                        116, 0, 0, 0,
                    ],
                })
            }
            TxModifier::WrongSignature => {
                Halt::ValidationFailed(VmRevertReason::General {
                    msg: "Account validation returned invalid magic value. Most often this means that the signature is incorrect".to_string(),
                    data: vec![],
                })
            }
            TxModifier::WrongMagicValue => {
                Halt::ValidationFailed(VmRevertReason::General {
                    msg: "v is neither 27 nor 28".to_string(),
                    data: vec![
                        8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 22, 118, 32, 105, 115, 32, 110, 101, 105, 116, 104,
                        101, 114, 32, 50, 55, 32, 110, 111, 114, 32, 50, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    ],
                })

            }
            TxModifier::WrongNonce => {
                Halt::ValidationFailed(VmRevertReason::General {
                    msg: "Incorrect nonce".to_string(),
                    data: vec![
                        8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 73, 110, 99, 111, 114, 114, 101, 99, 116, 32, 110,
                        111, 110, 99, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    ],
                })
            }
            TxModifier::NonceReused => {
                Halt::ValidationFailed(VmRevertReason::General {
                    msg: "Reusing the same nonce twice".to_string(),
                    data: vec![
                        8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 82, 101, 117, 115, 105, 110, 103, 32, 116, 104,
                        101, 32, 115, 97, 109, 101, 32, 110, 111, 110, 99, 101, 32, 116, 119, 105, 99, 101, 0,
                        0, 0, 0,
                    ],
                })
            }
        };

        ExpectedError {
            revert_reason: TxRevertReason::Halt(revert_reason),
            modifier: Some(value),
        }
    }
}

impl TransactionTestInfo {
    pub(crate) fn new_rejected(
        mut transaction: Transaction,
        expected_error: ExpectedError,
    ) -> Self {
        transaction.common_data = match transaction.common_data {
            ExecuteTransactionCommon::L2(mut data) => {
                if let Some(modifier) = &expected_error.modifier {
                    match modifier {
                        TxModifier::WrongSignatureLength => {
                            data.signature = data.signature[..data.signature.len() - 20].to_vec()
                        }
                        TxModifier::WrongSignature => data.signature = vec![27u8; 65],
                        TxModifier::WrongMagicValue => data.signature = vec![1u8; 65],
                        TxModifier::WrongNonce => {
                            // Do not need to modify signature for nonce error
                        }
                        TxModifier::NonceReused => {
                            // Do not need to modify signature for nonce error
                        }
                    }
                }
                ExecuteTransactionCommon::L2(data)
            }
            _ => panic!("L1 transactions are not supported"),
        };

        Self {
            tx: transaction,
            result: TxExpectedResult::Rejected {
                error: expected_error,
            },
        }
    }

    pub(crate) fn new_processed(transaction: Transaction, should_be_rollbacked: bool) -> Self {
        Self {
            tx: transaction,
            result: TxExpectedResult::Processed {
                rollback: should_be_rollbacked,
            },
        }
    }

    fn verify_result(&self, result: &VmExecutionResultAndLogs) {
        match &self.result {
            TxExpectedResult::Rejected { error } => match &result.result {
                ExecutionResult::Success { .. } => {
                    panic!("Transaction should be reverted {:?}", self.tx.nonce())
                }
                ExecutionResult::Revert { output } => match &error.revert_reason {
                    TxRevertReason::TxReverted(expected) => {
                        assert_eq!(output, expected)
                    }
                    _ => {
                        panic!("Error types mismatch");
                    }
                },
                ExecutionResult::Halt { reason } => match &error.revert_reason {
                    TxRevertReason::Halt(expected) => {
                        assert_eq!(reason, expected)
                    }
                    _ => {
                        panic!("Error types mismatch");
                    }
                },
            },
            TxExpectedResult::Processed { .. } => {
                assert!(!result.result.is_failed());
            }
        }
    }

    fn should_rollback(&self) -> bool {
        match &self.result {
            TxExpectedResult::Rejected { .. } => true,
            TxExpectedResult::Processed { rollback } => *rollback,
        }
    }
}

// TODO this doesn't include all the state of ModifiedWorld
#[derive(Debug, PartialEq)]
struct VmStateDump {
    state: vm2::State<CircuitsTracer>,
    storage_writes: Vec<((H160, U256), U256)>,
    events: Box<[vm2::Event]>,
}

impl<S: ReadStorage> Vm<S> {
    fn dump_state(&self) -> VmStateDump {
        VmStateDump {
            state: self.inner.state.clone(),
            storage_writes: self
                .inner
                .world_diff
                .get_storage_state()
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect(),
            events: self.inner.world_diff.events().into(),
        }
    }
}

impl VmTester {
    pub(crate) fn execute_and_verify_txs(
        &mut self,
        txs: &[TransactionTestInfo],
    ) -> CurrentExecutionState {
        for tx_test_info in txs {
            self.execute_tx_and_verify(tx_test_info.clone());
        }
        self.vm.execute(VmExecutionMode::Batch);
        let mut state = self.vm.get_current_execution_state();
        state.used_contract_hashes.sort();
        state
    }

    pub(crate) fn execute_tx_and_verify(
        &mut self,
        tx_test_info: TransactionTestInfo,
    ) -> VmExecutionResultAndLogs {
        self.vm.make_snapshot();
        let inner_state_before = self.vm.dump_state();
        self.vm.push_transaction(tx_test_info.tx.clone());
        let result = self.vm.execute(VmExecutionMode::OneTx);
        tx_test_info.verify_result(&result);
        if tx_test_info.should_rollback() {
            self.vm.rollback_to_the_latest_snapshot();
            let inner_state_after = self.vm.dump_state();
            pretty_assertions::assert_eq!(
                inner_state_before,
                inner_state_after,
                "Inner state before and after rollback should be equal"
            );
        } else {
            self.vm.pop_snapshot_no_rollback();
        }
        result
    }
}
