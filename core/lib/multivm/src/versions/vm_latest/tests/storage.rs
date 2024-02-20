use ethabi::Token;
use zksync_contracts::{load_contract, read_bytecode};
use zksync_system_constants::L2_ETH_TOKEN_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{
    get_code_key, get_known_code_key, get_nonce_key,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    AccountTreeId, Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::{get_balance, read_test_contract, verify_required_storage},
        },
        utils::fee::get_batch_base_fee,
        HistoryEnabled,
    },
    vm_m5::storage::Storage,
};

fn test_pubdata_counter(calldata: Vec<u8>) -> u32 {
    let bytecode = read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/pubdata-counter/pubdata-counter.sol/PubdataCounter.json");

    let test_contract_address = Address::random();

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_deployer()
        .with_random_rich_accounts(1)
        .with_custom_contracts(vec![(bytecode, test_contract_address, false)])
        .build();

    // We need to do it just to ensure that the tracers will not include stats from the start of the bootloader and will only include those for the transaction itself.
    vm.deploy_test_contract();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_contract_address),
            calldata,
            value: 0.into(),
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);

    result.statistics.pubdata_published
}

#[test]
fn test_storage_behavior() {
    let contract = load_contract("etc/contracts-test-data/artifacts-zk/contracts/pubdata-counter/pubdata-counter.sol/PubdataCounter.json");

    let base_pubdata = test_pubdata_counter(vec![]);
    let simple_test_pubdata = test_pubdata_counter(
        contract
            .function("simpleWrite")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_pubdata = test_pubdata_counter(
        contract
            .function("resettingWrite")
            .unwrap()
            .encode_input(&[])
            .unwrap(),
    );
    let resetting_write_via_revert_pubdata = test_pubdata_counter(
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
