use zksync_types::{Execute, Nonce};

use crate::{
    interface::{
        ExecutionResult, Halt, TxExecutionMode, TxRevertReason, VmExecutionMode, VmInterface,
        VmRevertReason,
    },
    vm_latest::{
        tests::{
            tester::{Account, VmTesterBuilder},
            utils::read_nonce_holder_tester,
        },
        types::internals::TransactionData,
        HistoryEnabled,
    },
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

#[test]
fn test_nonce_holder() {
    let mut account = Account::random();

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_deployer()
        .with_custom_contracts(vec![(
            read_nonce_holder_tester().to_vec(),
            account.address,
            true,
        )])
        .with_rich_accounts(vec![account.clone()])
        .build();

    // TODO
    let mut _run_nonce_test = |nonce: u32,
                               test_mode: NonceHolderTestMode,
                               error_message: Option<String>,
                               comment: &'static str| {
        // In this test we have to reset VM state after each test case. Because once bootloader failed during the validation of the transaction,
        // it will fail again and again. At the same time we have to keep the same storage, because we want to keep the nonce holder contract state.
        // The easiest way in terms of lifetimes is to reuse `vm_builder` to achieve it.
        vm.reset_state(true);
        let mut transaction_data: TransactionData = account
            .get_l2_tx_for_execute_with_nonce(
                Execute {
                    contract_address: account.address,
                    calldata: vec![12],
                    value: Default::default(),
                    factory_deps: vec![],
                },
                None,
                Nonce(nonce),
            )
            .into();

        transaction_data.signature = vec![test_mode.into()];
        vm.vm.push_raw_transaction(transaction_data, 0, 0, true);
        let result = vm.vm.execute(VmExecutionMode::OneTx);

        if let Some(msg) = error_message {
            let expected_error =
                TxRevertReason::Halt(Halt::ValidationFailed(VmRevertReason::General {
                    msg,
                    data: vec![],
                }));
            let ExecutionResult::Halt { reason } = result.result else {
                panic!("Expected revert, got {:?}", result.result);
            };
            assert_eq!(
                reason.to_string(),
                expected_error.to_string(),
                "{}",
                comment
            );
        } else {
            assert!(!result.result.is_failed(), "{}", comment);
        }
    };
    // TODO reenable.
    // // Test 1: trying to set value under non sequential nonce value.
    // run_nonce_test(
    //     1u32,
    //     NonceHolderTestMode::SetValueUnderNonce,
    //     Some("Previous nonce has not been used".to_string()),
    //     "Allowed to set value under non sequential value",
    // );

    // // Test 2: increase min nonce by 1 with sequential nonce ordering:
    // run_nonce_test(
    //     0u32,
    //     NonceHolderTestMode::IncreaseMinNonceBy1,
    //     None,
    //     "Failed to increment nonce by 1 for sequential account",
    // );

    // // Test 3: correctly set value under nonce with sequential nonce ordering:
    // run_nonce_test(
    //     1u32,
    //     NonceHolderTestMode::SetValueUnderNonce,
    //     None,
    //     "Failed to set value under nonce sequential value",
    // );

    // // Test 5: migrate to the arbitrary nonce ordering:
    // run_nonce_test(
    //     2u32,
    //     NonceHolderTestMode::SwitchToArbitraryOrdering,
    //     None,
    //     "Failed to switch to arbitrary ordering",
    // );

    // // Test 6: increase min nonce by 5
    // run_nonce_test(
    //     6u32,
    //     NonceHolderTestMode::IncreaseMinNonceBy5,
    //     None,
    //     "Failed to increase min nonce by 5",
    // );

    // // Test 7: since the nonces in range [6,10] are no longer allowed, the
    // // tx with nonce 10 should not be allowed
    // run_nonce_test(
    //     10u32,
    //     NonceHolderTestMode::IncreaseMinNonceBy5,
    //     Some("Reusing the same nonce twice".to_string()),
    //     "Allowed to reuse nonce below the minimal one",
    // );

    // // Test 8: we should be able to use nonce 13
    // run_nonce_test(
    //     13u32,
    //     NonceHolderTestMode::SetValueUnderNonce,
    //     None,
    //     "Did not allow to use unused nonce 10",
    // );

    // // Test 9: we should not be able to reuse nonce 13
    // run_nonce_test(
    //     13u32,
    //     NonceHolderTestMode::IncreaseMinNonceBy5,
    //     Some("Reusing the same nonce twice".to_string()),
    //     "Allowed to reuse the same nonce twice",
    // );

    // // Test 10: we should be able to simply use nonce 14, while bumping the minimal nonce by 5
    // run_nonce_test(
    //     14u32,
    //     NonceHolderTestMode::IncreaseMinNonceBy5,
    //     None,
    //     "Did not allow to use a bumped nonce",
    // );

    // // Test 11: Do not allow bumping nonce by too much
    // run_nonce_test(
    //     16u32,
    //     NonceHolderTestMode::IncreaseMinNonceTooMuch,
    //     Some("The value for incrementing the nonce is too high".to_string()),
    //     "Allowed for incrementing min nonce too much",
    // );

    // // Test 12: Do not allow not setting a nonce as used
    // run_nonce_test(
    //     16u32,
    //     NonceHolderTestMode::LeaveNonceUnused,
    //     Some("The nonce was not set as used".to_string()),
    //     "Allowed to leave nonce as unused",
    // );
}
