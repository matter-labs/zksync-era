use std::str::FromStr;

use ethabi::{Contract, Token};
use zk_evm_1_5_0::zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32};
use zksync_contracts::{deployer_contract, BaseSystemContracts, SystemContractCode};
use zksync_state::InMemoryStorage;
use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, L2_ETH_TOKEN_ADDRESS};
use zksync_types::{
    get_code_key, get_deployer_key, get_known_code_key, get_nonce_key,
    system_contracts::{DEPLOYMENT_NONCE_INCREMENT, TX_NONCE_INCREMENT},
    utils::deployed_address_evm_create,
    web3::signing::keccak256,
    AccountTreeId, Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{
    address_to_h256, bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256,
};

use super::tester::VmTester;
use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::{
                get_balance, key_for_evm_hash, read_test_contract, read_test_evm_simulator,
                verify_required_storage,
            },
        },
        utils::fee::get_batch_base_fee,
        HistoryEnabled,
    },
    vm_m5::storage::Storage,
};

const EXPECTED_EVM_STIPEND: u32 = (1 << 30);

fn set_up_evm_simulator_contract() -> (VmTester<HistoryEnabled>, Address, Contract) {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    // Just some address in user space
    let test_address = Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap();

    // We don't even need the preimage of it, just need the correct format to trigger the simulator
    let sample_evm_bytecode_hash = {
        let mut hash = [0u8; 32];
        hash[0] = BlobSha256Format::VERSION_BYTE;
        assert!(BlobSha256Format::is_valid(&hash));
        H256(hash)
    };

    storage.set_value(get_code_key(&test_address), sample_evm_bytecode_hash);

    let (evm_simulator_bytecode, evm_simualator) = read_test_evm_simulator();
    let evm_simulator_bytecode_hash = hash_bytecode(&evm_simulator_bytecode);

    storage.set_value(
        get_deployer_key(u256_to_h256(1.into())),
        evm_simulator_bytecode_hash,
    );
    let mut base_system_contracts = BaseSystemContracts::playground();
    base_system_contracts.evm_simulator = SystemContractCode {
        hash: evm_simulator_bytecode_hash,
        code: bytes_to_be_words(evm_simulator_bytecode.clone()),
    };

    storage.store_factory_dep(
        base_system_contracts.evm_simulator.hash,
        evm_simulator_bytecode,
    );

    let vm: crate::vm_latest::tests::tester::VmTester<HistoryEnabled> =
        VmTesterBuilder::new(HistoryEnabled)
            .with_storage(storage)
            .with_base_system_smart_contracts(base_system_contracts)
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_random_rich_accounts(1)
            .build();

    (vm, test_address, evm_simualator)
}

#[test]
fn test_evm_simulator_get_gas() {
    // We check that simulator receives the stipend
    let (mut vm, test_address, evm_simulator) = set_up_evm_simulator_contract();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: evm_simulator
                .function("getGas")
                .unwrap()
                .encode_input(&[])
                .unwrap(),
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
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");

    let saved_value = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(test_address),
        H256::zero(),
    ));
    let gas_received = h256_to_u256(saved_value).as_u32();

    assert!(gas_received > EXPECTED_EVM_STIPEND, "Stipend wasnt applied");
}

fn test_gas_conversion(gas_to_pass: u32, ratio: u32) -> u32 {
    // We check that simulator can correctly calculate how much "real" gas was passed to it
    let (mut vm, test_address, evm_simulator) = set_up_evm_simulator_contract();

    let account = &mut vm.rich_accounts[0];

    // It is of course possible that a bit of gas will be spent on the initial functionality
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: evm_simulator
                .function("testGas")
                .unwrap()
                .encode_input(&[
                    Token::Uint(U256::from(gas_to_pass)),
                    Token::Uint(U256::from(ratio)),
                ])
                .unwrap(),
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
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");

    let saved_value = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(test_address),
        H256::zero(),
    ));

    h256_to_u256(saved_value).as_u32()
}

#[test]
fn test_evm_simulator_gas_conversion() {
    // We check that there is a constant factor for getitng the EVM gas.
    let gas_obtained = test_gas_conversion(100_000, 100);
    assert!(gas_obtained >= 995, "Gas obtained is too low");
    assert!(gas_obtained <= 1000, "Gas obtained is too high");

    let gas_obtained_large = test_gas_conversion(1_000_000, 100);
    assert!(gas_obtained_large >= 9950, "Gas obtained is too low");
    assert!(gas_obtained_large <= 10000, "Gas obtained is too high");

    assert_eq!(
        1000 - gas_obtained,
        10000 - gas_obtained_large,
        "The difference between gas obtained is constant"
    );
}

#[test]
fn test_evm_simulator_static_call() {
    // EVM simulator should detect + circuimvent static context requirements
    let (mut vm, test_address, evm_simulator) = set_up_evm_simulator_contract();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: evm_simulator
                .function("testStaticCall")
                .unwrap()
                .encode_input(&[])
                .unwrap(),
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
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");

    let saved_value = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(test_address),
        H256::zero(),
    ));

    // The contract will store `1` in case the test was successful
    assert_eq!(
        h256_to_u256(saved_value),
        U256::one(),
        "Static call wasn't successful"
    );
}

// TODO: this test does not work
#[test]
fn test_evm_simulator_returndata_caching() {
    // EVM simulator should use active ptr to remember the correct returndata
    let (mut vm, test_address, evm_simulator) = set_up_evm_simulator_contract();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: evm_simulator
                .function("testRememberingReturndata")
                .unwrap()
                .encode_input(&[])
                .unwrap(),
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
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");
}

#[test]
fn test_evm_simulator_constructor() {
    // EVM simulator should use active ptr to remember the correct returndata
    let (mut vm, _, _) = set_up_evm_simulator_contract();

    let account = &mut vm.rich_accounts[0];

    let sample_evm_code = hex::decode("ffffffffffffffeeeeeeeabadbab").unwrap();
    let evm_code_hash = H256(keccak256(&sample_evm_code));

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: None,
            calldata: sample_evm_code,
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
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");

    let expected_deployed_address = deployed_address_evm_create(account.address, 0.into());

    let stored_var = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(expected_deployed_address),
        H256::zero(),
    ));

    let stored_evm_code_hash = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(CONTRACT_DEPLOYER_ADDRESS),
        key_for_evm_hash(&expected_deployed_address),
    ));

    assert_eq!(
        stored_evm_code_hash, evm_code_hash,
        "EVM code hash wasn't stored correctly"
    );
}
