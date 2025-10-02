use ethabi::Token;
use zksync_test_contracts::TestContract;
use zksync_types::{u256_to_h256, utils::storage_key_for_eth_balance, Address, Execute, U256};

use super::{
    default_pubdata_builder, get_empty_storage, tester::VmTesterBuilder, ContractToDeploy, TestedVm,
};
use crate::interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt};

enum TestOptions {
    Send(U256),
    Transfer(U256),
}

fn test_send_or_transfer<VM: TestedVm>(test_option: TestOptions) {
    let test_contract = TestContract::transfer_test();
    let test_contract_address = Address::repeat_byte(1);
    let recipient_address = Address::repeat_byte(2);

    let (value, calldata) = match test_option {
        TestOptions::Send(value) => (
            value,
            test_contract
                .function("send")
                .encode_input(&[Token::Address(recipient_address), Token::Uint(value)])
                .unwrap(),
        ),
        TestOptions::Transfer(value) => (
            value,
            test_contract
                .function("transfer")
                .encode_input(&[Token::Address(recipient_address), Token::Uint(value)])
                .unwrap(),
        ),
    };

    let mut storage = get_empty_storage();
    storage.set_value(
        storage_key_for_eth_balance(&test_contract_address),
        u256_to_h256(value),
    );

    let mut vm = VmTesterBuilder::new()
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![
            ContractToDeploy::new(
                TestContract::transfer_test().bytecode.to_vec(),
                test_contract_address,
            ),
            ContractToDeploy::new(
                TestContract::transfer_recipient().bytecode.to_vec(),
                recipient_address,
            ),
        ])
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_contract_address),
            calldata,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let tx_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let batch_result = vm
        .vm
        .finish_batch(default_pubdata_builder())
        .block_tip_execution_result;
    assert!(
        !batch_result.result.is_failed(),
        "Batch wasn't successful: {batch_result:#?}"
    );

    let new_recipient_balance = vm.get_eth_balance(recipient_address);
    assert_eq!(new_recipient_balance, value);
}

pub(crate) fn test_send_and_transfer<VM: TestedVm>() {
    test_send_or_transfer::<VM>(TestOptions::Send(U256::zero()));
    test_send_or_transfer::<VM>(TestOptions::Send(U256::from(10).pow(18.into())));
    test_send_or_transfer::<VM>(TestOptions::Transfer(U256::zero()));
    test_send_or_transfer::<VM>(TestOptions::Transfer(U256::from(10).pow(18.into())));
}

fn test_reentrancy_protection_send_or_transfer<VM: TestedVm>(test_option: TestOptions) {
    let test_contract = TestContract::transfer_test();
    let reentrant_recipient_contract = TestContract::reentrant_recipient();
    let test_contract_address = Address::repeat_byte(1);
    let reentrant_recipient_address = Address::repeat_byte(2);

    let (value, calldata) = match test_option {
        TestOptions::Send(value) => (
            value,
            test_contract
                .function("send")
                .encode_input(&[
                    Token::Address(reentrant_recipient_address),
                    Token::Uint(value),
                ])
                .unwrap(),
        ),
        TestOptions::Transfer(value) => (
            value,
            test_contract
                .function("transfer")
                .encode_input(&[
                    Token::Address(reentrant_recipient_address),
                    Token::Uint(value),
                ])
                .unwrap(),
        ),
    };

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![
            ContractToDeploy::new(
                TestContract::transfer_test().bytecode.to_vec(),
                test_contract_address,
            ),
            ContractToDeploy::new(
                TestContract::reentrant_recipient().bytecode.to_vec(),
                reentrant_recipient_address,
            ),
        ])
        .build::<VM>();

    // First transaction, the job of which is to warm up the slots for balance of the recipient as well as its storage variable.
    let account = &mut vm.rich_accounts[0];
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(reentrant_recipient_address),
            calldata: reentrant_recipient_contract
                .function("setX")
                .encode_input(&[])
                .unwrap(),
            value: U256::from(1),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx1);
    let tx1_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !tx1_result.result.is_failed(),
        "Transaction 1 wasn't successful"
    );

    let tx2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_contract_address),
            calldata,
            value,
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx2);
    let tx2_result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        tx2_result.result.is_failed(),
        "Transaction 2 should have failed, but it succeeded"
    );

    let batch_result = vm
        .vm
        .finish_batch(default_pubdata_builder())
        .block_tip_execution_result;
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");
}

pub(crate) fn test_reentrancy_protection_send_and_transfer<VM: TestedVm>() {
    test_reentrancy_protection_send_or_transfer::<VM>(TestOptions::Send(U256::zero()));
    test_reentrancy_protection_send_or_transfer::<VM>(TestOptions::Send(
        U256::from(10).pow(18.into()),
    ));
    test_reentrancy_protection_send_or_transfer::<VM>(TestOptions::Transfer(U256::zero()));
    test_reentrancy_protection_send_or_transfer::<VM>(TestOptions::Transfer(
        U256::from(10).pow(18.into()),
    ));
}
