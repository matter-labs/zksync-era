use std::str::FromStr;

use ethabi::{Contract, Token};
use itertools::Itertools;
// FIXME: 1.4.1 should not be imported from 1.5.0
use zk_evm_1_4_1::sha2::{self};
use zk_evm_1_5_0::zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32};
use zksync_contracts::{load_contract, read_evm_bytecode};
use zksync_state::InMemoryStorage;
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    get_address_mapping_key, get_code_key, get_deployer_key, get_evm_code_hash_key,
    utils::{deployed_address_evm_create, deployed_address_evm_create2},
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
        tracers::evm_debug_tracer::EvmDebugTracer,
        utils::fee::get_batch_base_fee,
        HistoryEnabled, ToTracerPointer, TracerDispatcher, TracerPointer,
    },
    vm_m5::storage::Storage,
    HistoryMode,
};

fn read_test_evm_bytecode(folder_name: &str, contract_name: &str) -> (Vec<u8>, Vec<u8>) {
    read_evm_bytecode(format!("etc/evm-contracts-test-data/artifacts/contracts/{folder_name}/{contract_name}.sol/{contract_name}.json"))
}

fn load_test_evm_contract(folder_name: &str, contract_name: &str) -> Contract {
    load_contract(format!(
        "etc/evm-contracts-test-data/artifacts/contracts/{folder_name}/{contract_name}.sol/{contract_name}.json",
    ))
}

fn hash_evm_bytecode(bytecode: Vec<u8>) -> H256 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let len = bytecode.len() as u16;
    hasher.update(bytecode);
    let result = hasher.finalize();

    let mut output = [0u8; 32];
    output[..].copy_from_slice(&result.as_slice());
    output[0] = BlobSha256Format::VERSION_BYTE;
    output[1] = 0;
    output[2..4].copy_from_slice(&len.to_be_bytes());

    H256(output)
}

fn test_evm_vector(mut bytecode: Vec<u8>) -> U256 {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    // To avoid problems with correct encoding for these tests, we just pad the bytecode to be divisible by 32.
    while bytecode.len() % 32 != 0 {
        bytecode.push(0);
    }

    let blob_hash = hash_evm_bytecode(bytecode.clone());
    assert!(BlobSha256Format::is_valid(&blob_hash.0));
    let evm_hash = H256(keccak256(&bytecode));

    // Just some address in user space
    let test_address = Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap();

    let code_key = get_address_mapping_key(&test_address, u256_to_h256(2.into()));
    let code_content_key = H256(keccak256(code_key.as_bytes()));

    // *2 + 1 is hte requiremnt for solidity storage layout when length > 31
    storage.set_value(
        get_deployer_key(code_key),
        u256_to_h256((bytecode.len() * 2 + 1).into()),
    );

    bytes_to_be_words(bytecode)
        .into_iter()
        .enumerate()
        .for_each(|(i, chunk)| {
            let key = h256_to_u256(code_content_key);
            storage.set_value(
                StorageKey::new(
                    AccountTreeId::new(CONTRACT_DEPLOYER_ADDRESS),
                    u256_to_h256(key + U256::from(i)),
                ),
                u256_to_h256(chunk),
            );
        });

    let evm_code_hash_key = get_evm_code_hash_key(&test_address);

    storage.set_value(get_code_key(&test_address), blob_hash);
    storage.set_value(evm_code_hash_key, evm_hash);

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: vec![],
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);

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

    h256_to_u256(saved_value)
}

#[test]
fn test_basic_evm_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 1 push32 0 sstore
                hex::decode("7f").unwrap(),
                u256_to_h256(1.into()).0.to_vec(),
                hex::decode("7f").unwrap(),
                H256::zero().0.to_vec(),
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        1.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 1
                hex::decode("7f").unwrap(),
                u256_to_h256(1.into()).0.to_vec(),
                // push4 15
                hex::decode("63").unwrap(),
                hex::decode("0000000f").unwrap(),
                // add
                hex::decode("01").unwrap(),
                // push1 2
                hex::decode("60").unwrap(),
                hex::decode("02").unwrap(),
                // mul
                hex::decode("02").unwrap(),
                // push0
                hex::decode("5f").unwrap(),
                // binor
                hex::decode("17").unwrap(),
                // push0
                hex::decode("5f").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        32.into()
    );
}

fn assert_deployed_hash<H: HistoryMode>(
    tester: &mut VmTester<H>,
    address: Address,
    expected_deployed_code_hash: H256,
) {
    let stored_evm_code_hash = tester.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(CONTRACT_DEPLOYER_ADDRESS),
        key_for_evm_hash(&address),
    ));
    assert_eq!(
        stored_evm_code_hash, expected_deployed_code_hash,
        "EVM code hash wasn't stored correctly"
    );
}

fn deploy_evm_contract<H: HistoryMode>(
    tester: &mut VmTester<H>,
    folder_name: &str,
    contract_name: &str,
) -> (Address, Contract) {
    let account = &mut tester.rich_accounts[0];

    let (counter_bytecode, counter_deployed_bytecode) =
        read_test_evm_bytecode(folder_name, contract_name);
    let abi = load_test_evm_contract(folder_name, contract_name);

    let sample_evm_code = counter_bytecode;
    let expected_deployed_code_hash = H256(keccak256(&counter_deployed_bytecode));

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: None,
            calldata: sample_evm_code,
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );

    tester.vm.push_transaction(tx);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs = tester.vm.inspect(
        EvmDebugTracer::new(Address::default())
            .into_tracer_pointer()
            .into(),
        VmExecutionMode::OneTx,
    );

    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let expected_deployed_address = deployed_address_evm_create(account.address, U256::zero());
    assert_deployed_hash(
        tester,
        expected_deployed_address,
        expected_deployed_code_hash,
    );

    (expected_deployed_address, abi)
}

#[test]
fn test_basic_evm_interaction() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let (expected_deployed_address, abi) = deploy_evm_contract(&mut vm, "counter", "Counter");
    let account = &mut vm.rich_accounts[0];

    let tx2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(expected_deployed_address),
            calldata: abi
                .function("increment")
                .unwrap()
                .encode_input(&[Token::Uint(U256::from(15))])
                .unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );
    vm.vm.push_transaction(tx2);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let tx3 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(expected_deployed_address),
            calldata: abi
                .function("increment")
                .unwrap()
                .encode_input(&[Token::Uint(U256::from(35))])
                .unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );
    vm.vm.push_transaction(tx3);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let batch_result = vm.vm.execute(VmExecutionMode::Batch);
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");

    let saved_value = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(expected_deployed_address),
        H256::zero(),
    ));
    assert_eq!(h256_to_u256(saved_value), U256::from(50));
}

#[test]
fn test_evm_gas_consumption() {
    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let (expected_deployed_address, abi) = deploy_evm_contract(&mut vm, "gas-tester", "GasTester");
    println!("Deployed address: {:?}", expected_deployed_address);

    let account = &mut vm.rich_accounts[0];

    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(expected_deployed_address),
            calldata: abi.function("testGas").unwrap().encode_input(&[]).unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );
    vm.vm.push_transaction(tx1);
    let tx_result = vm.vm.inspect(
        EvmDebugTracer::new(expected_deployed_address)
            .into_tracer_pointer()
            .into(),
        VmExecutionMode::OneTx,
    );
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );
}

#[test]
fn test_evm_basic_create() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let (factory_address, _) = deploy_evm_contract(&mut vm, "create", "Import");

    // When the "Import" contract is deployed, it will create a new contract "Foo", so we just double check that it has also been deployed

    let (foo_constructor_bytecode, foo_deployed_bytecode) = read_test_evm_bytecode("create", "Foo");

    let expected_deployed_code_hash = H256(keccak256(&foo_deployed_bytecode));

    assert_deployed_hash(
        &mut vm,
        deployed_address_evm_create(factory_address, U256::zero()),
        expected_deployed_code_hash,
    );

    assert_deployed_hash(
        &mut vm,
        deployed_address_evm_create2(
            factory_address,
            H256::zero(),
            H256(keccak256(&foo_constructor_bytecode)),
        ),
        expected_deployed_code_hash,
    )
}
