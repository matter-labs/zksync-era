use zksync_test_account::Account;
use zksync_types::{Execute, ExecuteTransactionCommon, Nonce};

use super::{read_nonce_holder_tester, tester::VmTesterBuilder, ContractToDeploy, TestedVm};
use crate::interface::{
    ExecutionResult, Halt, InspectExecutionMode, TxExecutionMode, TxRevertReason, VmInterfaceExt,
    VmRevertReason,
};

pub enum NonceHolderTestMode {
    SetValueUnderNonce,
    IncreaseMinNonceBy5,
    IncreaseMinNonceTooMuch,
    LeaveNonceUnused,
    IncreaseMinNonceBy1,
    SwitchToArbitraryOrdering,
}

impl From<NonceHolderTestMode> for u8 {
    fn from(mode: NonceHolderTestMode) -> u8 {
        match mode {
            NonceHolderTestMode::SetValueUnderNonce => 0,
            NonceHolderTestMode::IncreaseMinNonceBy5 => 1,
            NonceHolderTestMode::IncreaseMinNonceTooMuch => 2,
            NonceHolderTestMode::LeaveNonceUnused => 3,
            NonceHolderTestMode::IncreaseMinNonceBy1 => 4,
            NonceHolderTestMode::SwitchToArbitraryOrdering => 5,
        }
    }
}

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
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1);
    let account_address = builder.rich_account(0).address;
    let mut vm = builder
        .with_custom_contracts(vec![ContractToDeploy::account(
            read_nonce_holder_tester(),
            account_address,
        )])
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let hex_addr = hex::encode(account.address.to_fixed_bytes());

    // Test 1: trying to set value under non sequential nonce value.
    run_nonce_test(
        &mut vm.vm,
        account,
        1u32,
        NonceHolderTestMode::SetValueUnderNonce,
        Some("Error function_selector = 0x13595475, data = 0x13595475".to_string()),
        "Allowed to set value under non sequential value",
    );

    // Test 2: increase min nonce by 1 with sequential nonce ordering:
    run_nonce_test(
        &mut vm.vm,
        account,
        0u32,
        NonceHolderTestMode::IncreaseMinNonceBy1,
        None,
        "Failed to increment nonce by 1 for sequential account",
    );

    // Test 3: correctly set value under nonce with sequential nonce ordering:
    run_nonce_test(
        &mut vm.vm,
        account,
        1u32,
        NonceHolderTestMode::SetValueUnderNonce,
        None,
        "Failed to set value under nonce sequential value",
    );

    // Test 5: migrate to the arbitrary nonce ordering:
    run_nonce_test(
        &mut vm.vm,
        account,
        2u32,
        NonceHolderTestMode::SwitchToArbitraryOrdering,
        None,
        "Failed to switch to arbitrary ordering",
    );

    // Test 6: increase min nonce by 5
    run_nonce_test(
        &mut vm.vm,
        account,
        6u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        None,
        "Failed to increase min nonce by 5",
    );

    // Test 7: since the nonces in range [6,10] are no longer allowed, the
    // tx with nonce 10 should not be allowed
    run_nonce_test(
        &mut vm.vm,
        account,
        10u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        Some(format!("Error function_selector = 0xe90aded4, data = 0xe90aded4000000000000000000000000{hex_addr}000000000000000000000000000000000000000000000000000000000000000a")),
        "Allowed to reuse nonce below the minimal one",
    );

    // Test 8: we should be able to use nonce 13
    run_nonce_test(
        &mut vm.vm,
        account,
        13u32,
        NonceHolderTestMode::SetValueUnderNonce,
        None,
        "Did not allow to use unused nonce 10",
    );

    // Test 9: we should not be able to reuse nonce 13
    run_nonce_test(
        &mut vm.vm,
        account,
        13u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        Some(format!("Error function_selector = 0xe90aded4, data = 0xe90aded4000000000000000000000000{hex_addr}000000000000000000000000000000000000000000000000000000000000000d")),
        "Allowed to reuse the same nonce twice",
    );

    // Test 10: we should be able to simply use nonce 14, while bumping the minimal nonce by 5
    run_nonce_test(
        &mut vm.vm,
        account,
        14u32,
        NonceHolderTestMode::IncreaseMinNonceBy5,
        None,
        "Did not allow to use a bumped nonce",
    );

    // Test 11: Do not allow bumping nonce by too much
    run_nonce_test(
        &mut vm.vm,
        account,
        16u32,
        NonceHolderTestMode::IncreaseMinNonceTooMuch,
        Some("Error function_selector = 0x45ac24a6, data = 0x45ac24a600000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000040000000000000000000000".to_string()),
        "Allowed for incrementing min nonce too much",
    );

    // Test 12: Do not allow not setting a nonce as used
    run_nonce_test(
        &mut vm.vm,
        account,
        16u32,
        NonceHolderTestMode::LeaveNonceUnused,
        Some(format!("Error function_selector = 0x1f2f8478, data = 0x1f2f8478000000000000000000000000{hex_addr}0000000000000000000000000000000000000000000000000000000000000010")),
        "Allowed to leave nonce as unused",
    );
}
