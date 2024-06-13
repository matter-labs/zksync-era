use ethabi::Token;
use zksync_contracts::{load_contract, read_bytecode};
use zksync_test_account::Account;
use zksync_types::{fee::Fee, Address, Execute, U256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface, VmInterfaceHistoryEnabled},
    vm_latest::{tests::tester::VmTesterBuilder, HistoryEnabled},
};

#[derive(Debug, Default)]

struct TestTxInfo {
    calldata: Vec<u8>,
    fee_overrides: Option<Fee>,
    should_fail: bool,
}

fn test_storage(txs: Vec<TestTxInfo>) -> u32 {
    let bytecode = read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    let test_contract_address = Address::random();

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_deployer()
        .with_random_rich_accounts(txs.len() as u32)
        .with_custom_contracts(vec![(bytecode, test_contract_address, false)])
        .build();

    let mut last_result = None;

    for (id, tx) in txs.into_iter().enumerate() {
        let TestTxInfo {
            calldata,
            fee_overrides,
            should_fail,
        } = tx;

        let account = &mut vm.rich_accounts[id];

        vm.vm.make_snapshot();

        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: test_contract_address,
                calldata,
                value: 0.into(),
                factory_deps: vec![],
            },
            fee_overrides,
        );

        vm.vm.push_transaction(tx);
        let result = vm.vm.execute(VmExecutionMode::OneTx);
        if should_fail {
            assert!(result.result.is_failed(), "Transaction should fail");
            vm.vm.rollback_to_the_latest_snapshot();
        } else {
            assert!(!result.result.is_failed(), "Transaction should not fail");
            vm.vm.pop_snapshot_no_rollback();
        }

        last_result = Some(result);
    }

    last_result.unwrap().statistics.pubdata_published
}

fn test_storage_one_tx(second_tx_calldata: Vec<u8>) -> u32 {
    test_storage(vec![
        TestTxInfo::default(),
        TestTxInfo {
            calldata: second_tx_calldata,
            fee_overrides: None,
            should_fail: false,
        },
    ])
}

#[test]
fn test_storage_behavior() {
    let contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    // In all of the tests below we provide the first tx to ensure that the tracers will not include
    // the statistics from the start of the bootloader and will only include those for the transaction itself.

    let base_pubdata = test_storage_one_tx(vec![]);
    let simple_test_pubdata = test_storage_one_tx(
        contract
            .function("simpleWrite")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_pubdata = test_storage_one_tx(
        contract
            .function("resettingWrite")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_via_revert_pubdata = test_storage_one_tx(
        contract
            .function("resettingWriteViaRevert")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );

    assert_eq!(simple_test_pubdata - base_pubdata, 65);
    assert_eq!(resetting_write_pubdata - base_pubdata, 34);
    assert_eq!(resetting_write_via_revert_pubdata - base_pubdata, 34);
}

#[test]
fn test_transient_storage_behavior() {
    let contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    let first_tstore_test = contract
        .function("testTransientStore")
        .unwrap()
        .encode_input(&[])
        .unwrap();
    // Second transaction checks that, as expected, the transient storage is cleared after the first transaction.
    let second_tstore_test = contract
        .function("assertTValue")
        .unwrap()
        .encode_input(&[Token::Uint(U256::zero())])
        .unwrap();

    test_storage(vec![
        TestTxInfo {
            calldata: first_tstore_test,
            ..TestTxInfo::default()
        },
        TestTxInfo {
            calldata: second_tstore_test,
            ..TestTxInfo::default()
        },
    ]);
}

#[test]
fn test_transient_storage_behavior_panic() {
    let contract = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json",
    );

    let basic_tstore_test = contract
        .function("tStoreAndRevert")
        .unwrap()
        .encode_input(&[Token::Uint(U256::one()), Token::Bool(false)])
        .unwrap();

    let small_fee = Fee {
        // Something very-very small to make the validation fail
        gas_limit: 10_000.into(),
        ..Account::default_fee()
    };

    test_storage(vec![
        TestTxInfo {
            calldata: basic_tstore_test.clone(),
            ..TestTxInfo::default()
        },
        TestTxInfo {
            fee_overrides: Some(small_fee),
            should_fail: true,
            ..TestTxInfo::default()
        },
        TestTxInfo {
            calldata: basic_tstore_test,
            ..TestTxInfo::default()
        },
    ]);
}
