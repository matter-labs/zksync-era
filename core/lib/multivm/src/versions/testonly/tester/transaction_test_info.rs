use zksync_types::{ExecuteTransactionCommon, Nonce, Transaction, H160};

use super::{TestedVm, VmTester};
use crate::{
    interface::{
        CurrentExecutionState, ExecutionResult, Halt, InspectExecutionMode, TxRevertReason,
        VmExecutionResultAndLogs, VmInterfaceExt, VmRevertReason,
    },
    versions::testonly::default_pubdata_builder,
};

#[derive(Debug, Clone)]
pub(crate) enum TxModifier {
    WrongSignatureLength,
    WrongSignature,
    WrongMagicValue,
    WrongNonce(Nonce, Nonce),
    NonceReused(H160, Nonce),
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
                Halt::ValidationFailed(VmRevertReason::Unknown {
                    function_selector: vec![144, 240, 73, 201],
                    data: vec![
                        144, 240, 73, 201, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 45
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
                Halt::ValidationFailed(VmRevertReason::Unknown {
                    function_selector: vec![144, 240, 73, 201],
                    data: vec![144, 240, 73, 201, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                })

            }
            TxModifier::WrongNonce(expected, actual) => {
                let function_selector = vec![98, 106, 222, 48];
                let expected_nonce_bytes = expected.0.to_be_bytes().to_vec();
                let actual_nonce_bytes = actual.0.to_be_bytes().to_vec();
                // padding is 28 because an address takes up 4 bytes and we need it to fill a 32 byte field
                let nonce_padding = vec![0u8; 28];
                let data = [function_selector.clone(), nonce_padding.clone(), expected_nonce_bytes, nonce_padding.clone(), actual_nonce_bytes].concat();
                Halt::ValidationFailed(VmRevertReason::Unknown {
                    function_selector,
                    data
                })
            }
            TxModifier::NonceReused(addr, nonce) => {
                let function_selector = vec![233, 10, 222, 212];
                let addr = addr.as_bytes().to_vec();
                // padding is 12 because an address takes up 20 bytes and we need it to fill a 32 byte field
                let addr_padding = vec![0u8; 12];
                // padding is 28 because an address takes up 4 bytes and we need it to fill a 32 byte field
                let nonce_padding = vec![0u8; 28];
                let data = [function_selector.clone(), addr_padding, addr, nonce_padding, nonce.0.to_be_bytes().to_vec()].concat();
                Halt::ValidationFailed(VmRevertReason::Unknown {
                    function_selector,
                    data,
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
                        TxModifier::WrongNonce(_, _) => {
                            // Do not need to modify signature for nonce error
                        }
                        TxModifier::NonceReused(_, _) => {
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

impl<VM: TestedVm> VmTester<VM> {
    pub(crate) fn execute_and_verify_txs(
        &mut self,
        txs: &[TransactionTestInfo],
    ) -> CurrentExecutionState {
        for tx_test_info in txs {
            self.execute_tx_and_verify(tx_test_info.clone());
        }
        self.vm.set_manual_l2_block_info();
        self.vm.finish_batch(default_pubdata_builder());
        let mut state = self.vm.get_current_execution_state();
        state.used_contract_hashes.sort();
        state
    }

    pub(crate) fn execute_tx_and_verify(
        &mut self,
        tx_test_info: TransactionTestInfo,
    ) -> VmExecutionResultAndLogs {
        execute_tx_and_verify(&mut self.vm, tx_test_info)
    }
}

fn execute_tx_and_verify(
    vm: &mut impl TestedVm,
    tx_test_info: TransactionTestInfo,
) -> VmExecutionResultAndLogs {
    let inner_state_before = vm.dump_state();
    vm.make_snapshot();
    vm.push_transaction(tx_test_info.tx.clone());
    let result = vm.execute(InspectExecutionMode::OneTx);
    tx_test_info.verify_result(&result);
    if tx_test_info.should_rollback() {
        vm.rollback_to_the_latest_snapshot();
        let inner_state_after = vm.dump_state();
        pretty_assertions::assert_eq!(
            inner_state_before,
            inner_state_after,
            "Inner state before and after rollback should be equal"
        );
    } else {
        vm.pop_snapshot_no_rollback();
    }
    result
}
