use zksync_test_contracts::{Account, TestContract};
use zksync_types::{Execute, ExecuteTransactionCommon, Nonce};

use super::{tester::VmTesterBuilder, ContractToDeploy, TestedVm};
use crate::interface::{
    ExecutionResult, Halt, InspectExecutionMode, TxExecutionMode, TxRevertReason, VmInterfaceExt,
    VmRevertReason,
};

pub enum NonceHolderTestMode {
    IncreaseMinNonceBy5,
    IncreaseMinNonceTooMuch,
    LeaveNonceUnused,
    IncreaseMinNonceBy1,
}

impl From<NonceHolderTestMode> for u8 {
    fn from(mode: NonceHolderTestMode) -> u8 {
        match mode {
            NonceHolderTestMode::IncreaseMinNonceBy5 => 1,
            NonceHolderTestMode::IncreaseMinNonceTooMuch => 2,
            NonceHolderTestMode::LeaveNonceUnused => 3,
            NonceHolderTestMode::IncreaseMinNonceBy1 => 4,
        }
    }
}

#[allow(dead_code)]
fn run_nonce_test(
    vm: &mut impl TestedVm,
    account: &mut Account,
    nonce: u32,
    test_mode: NonceHolderTestMode,
    error_message: Option<String>,
    comment: &'static str,
) {
    vm.make_snapshot();
    let mut transaction = account.get_l2_tx_for_execute_with_nonce(
        Execute {
            contract_address: Some(account.address),
            calldata: vec![12],
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
        Nonce(nonce),
    );
    let ExecuteTransactionCommon::L2(tx_data) = &mut transaction.common_data else {
        unreachable!();
    };
    tx_data.signature = vec![test_mode.into()];
    vm.push_transaction(transaction);
    let result = vm.execute(InspectExecutionMode::OneTx);

    if let Some(msg) = error_message {
        let expected_error =
            TxRevertReason::Halt(Halt::ValidationFailed(VmRevertReason::General {
                msg,
                data: vec![],
            }));
        let ExecutionResult::Halt { reason } = &result.result else {
            panic!("Expected revert, got {:?}", result.result);
        };
        assert_eq!(reason.to_string(), expected_error.to_string(), "{comment}");
        vm.rollback_to_the_latest_snapshot();
    } else {
        assert!(!result.result.is_failed(), "{}", comment);
        vm.pop_snapshot_no_rollback();
    }
}

pub(crate) fn test_nonce_holder<VM: TestedVm>() {
    let builder = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1);
    let account_address = builder.rich_account(0).address;
    let mut vm = builder
        .with_custom_contracts(vec![ContractToDeploy::account(
            TestContract::nonce_holder().bytecode.to_vec(),
            account_address,
        )])
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let hex_addr = hex::encode(account.address.to_fixed_bytes());

    // Test 1: increase min nonce by 1 with sequential nonce ordering:
    run_nonce_test(
        &mut vm.vm,
        account,
        0u32,
        NonceHolderTestMode::IncreaseMinNonceBy1,
        None,
        "Failed to increment nonce by 1 for sequential account",
    );

    // Test 2: increase min nonce by 5
    run_nonce_test(
        &mut vm.vm,
        account,
        1u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        None,
        "Failed to increase min nonce by 5",
    );

    // Test 3: since the nonces in range [2; 6] are no longer allowed, the
    // tx with nonce 5 should not be allowed
    run_nonce_test(
        &mut vm.vm,
        account,
        5u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        Some(format!("Error function_selector = 0xe90aded4, data = 0xe90aded4000000000000000000000000{hex_addr}0000000000000000000000000000000000000000000000000000000000000005")),
        "Allowed to reuse nonce below the minimal one",
    );

    // Test 4: we should be able to simply use nonce 10, while bumping the minimal nonce by 5
    run_nonce_test(
        &mut vm.vm,
        account,
        10u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        None,
        "Did not allow to use a bumped nonce",
    );

    // Test 5: Do not allow bumping nonce by too much
    run_nonce_test(
        &mut vm.vm,
        account,
        11u32,
        NonceHolderTestMode::IncreaseMinNonceTooMuch,
        Some("Error function_selector = 0xbac091ee, data = 0xbac091ee000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000040000000000000000000000".to_string()),
        "Allowed for incrementing min nonce too much",
    );

    // Test 6: Do not allow not setting a nonce as used
    run_nonce_test(
        &mut vm.vm,
        account,
        11u32,
        NonceHolderTestMode::LeaveNonceUnused,
        Some(format!("Error function_selector = 0x1f2f8478, data = 0x1f2f8478000000000000000000000000{hex_addr}000000000000000000000000000000000000000000000000000000000000000b")),
        "Allowed to leave nonce as unused",
    );
}
