use ethabi::Token;
use zksync_contracts::{load_contract, read_bytecode};
use zksync_system_constants::L2_ETH_TOKEN_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{
    get_code_key, get_known_code_key, get_nonce_key,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{get_empty_storage, DeployContractsTx, TxType, VmTesterBuilder},
            utils::{get_balance, read_test_contract, verify_required_storage},
        },
        utils::fee::get_batch_base_fee,
        HistoryEnabled,
    },
    vm_m5::storage::Storage,
};

enum TestOptions {
    Send(U256),
    Transfer(U256),
}

fn test_send_or_transfer(test_option: TestOptions) {
    let test_bytecode = read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/transfer/transfer.sol/TransferTest.json",
    );
    let recipeint_bytecode = read_bytecode(
        "etc/contracts-test-data/artifacts-zk/contracts/transfer/transfer.sol/Recipient.json",
    );
    let test_abi = load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/transfer/transfer.sol/TransferTest.json",
    );

    let test_contract_address = Address::random();
    let recipient_address = Address::random();

    let (value, calldata) = match test_option {
        TestOptions::Send(value) => (
            value,
            test_abi
                .function("send")
                .unwrap()
                .encode_input(&[Token::Address(recipient_address), Token::Uint(value)])
                .unwrap(),
        ),
        TestOptions::Transfer(value) => (
            value,
            test_abi
                .function("transfer")
                .unwrap()
                .encode_input(&[Token::Address(recipient_address), Token::Uint(value)])
                .unwrap(),
        ),
    };

    let mut storage = get_empty_storage();
    storage.set_value(
        storage_key_for_eth_balance(&test_contract_address),
        u256_to_h256(value),
    );

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_deployer()
        .with_random_rich_accounts(1)
        .with_custom_contracts(vec![
            (test_bytecode, test_contract_address, false),
            (recipeint_bytecode, recipient_address, false),
        ])
        .build();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: test_contract_address,
            calldata,
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let tx_result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let batch_result = vm.vm.execute(VmExecutionMode::Batch);
    assert!(
        !batch_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let new_recipient_balance = get_balance(
        AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
        &recipient_address,
        vm.vm.state.storage.storage.get_ptr(),
    );

    assert_eq!(new_recipient_balance, value);
}

#[test]
fn test_send_and_transfer() {
    test_send_or_transfer(TestOptions::Send(U256::zero()));
    test_send_or_transfer(TestOptions::Send(U256::from(10).pow(18.into())));
    test_send_or_transfer(TestOptions::Transfer(U256::zero()));
    test_send_or_transfer(TestOptions::Transfer(U256::from(10).pow(18.into())));
}
