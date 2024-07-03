use std::{
    env,
    error::Error,
    fs::{File, OpenOptions},
    io::{prelude::*, Seek, SeekFrom},
    ops::{Div, Sub},
    str::FromStr,
};

// FIXME: 1.4.1 should not be imported from 1.5.0
use chrono::{Datelike, Timelike, Utc};
use csv::{ReaderBuilder, Writer, WriterBuilder};
use ethabi::{encode, ethereum_types::H264, Contract, Token};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use zk_evm_1_4_1::sha2::{self};
use zk_evm_1_5_0::zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32};
use zksync_contracts::{load_contract, read_bytecode, read_evm_bytecode};
use zksync_state::{InMemoryStorage, StorageView};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    get_address_mapping_key, get_code_key, get_deployer_key, get_evm_code_hash_key,
    get_known_code_key,
    utils::{
        deployed_address_evm_create, deployed_address_evm_create2, storage_key_for_eth_balance,
    },
    web3::signing::keccak256,
    AccountTreeId, Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{
    address_to_h256, bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256,
};

use super::tester::VmTester;
use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_boojum_integration::tracers::dispatcher,
    vm_latest::{
        tests::{
            tester::{Account, DeployContractsTx, TxType, VmTesterBuilder},
            utils::{
                get_balance, key_for_evm_hash, load_test_evm_contract, read_erc20_contract,
                read_test_contract, read_test_evm_bytecode, read_test_evm_simulator,
                verify_required_storage,
            },
        },
        tracers::evm_debug_tracer::EvmDebugTracer,
        utils::{fee::get_batch_base_fee, hash_evm_bytecode},
        HistoryEnabled, ToTracerPointer, TracerDispatcher, TracerPointer,
    },
    vm_m5::storage::Storage,
    HistoryMode,
};
const CONTRACT_ADDRESS: &str = "0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7";

fn insert_evm_contract(storage: &mut InMemoryStorage, mut bytecode: Vec<u8>) -> Address {
    // To avoid problems with correct encoding for these tests, we just pad the bytecode to be divisible by 32.
    while bytecode.len() % 32 != 0 {
        bytecode.push(0);
    }

    let evm_hash = H256(keccak256(&bytecode));

    let padded_bytecode = {
        let mut padded_bytecode: Vec<u8> = vec![];

        let encoded_length = encode(&[Token::Uint(U256::from(bytecode.len()))]);

        padded_bytecode.extend(encoded_length);
        padded_bytecode.extend(bytecode.clone());

        while padded_bytecode.len() % 64 != 32 {
            padded_bytecode.push(0);
        }

        padded_bytecode
    };
    let blob_hash: H256 = hash_evm_bytecode(&padded_bytecode);

    assert!(BlobSha256Format::is_valid(&blob_hash.0));

    // Just some address in user space
    let test_address = Address::from_str(CONTRACT_ADDRESS).unwrap();

    let evm_code_hash_key = get_evm_code_hash_key(&test_address);

    storage.set_value(get_code_key(&test_address), blob_hash);
    storage.set_value(get_known_code_key(&blob_hash), u256_to_h256(U256::one()));

    storage.set_value(evm_code_hash_key, evm_hash);

    storage.store_factory_dep(blob_hash, padded_bytecode);

    // Marking bytecode as known

    test_address
}

fn insert_evm_contract_with_value(
    storage: &mut InMemoryStorage,
    mut bytecode: Vec<u8>,
    value: U256,
) -> Address {
    // To avoid problems with correct encoding for these tests, we just pad the bytecode to be divisible by 32.
    while bytecode.len() % 32 != 0 {
        bytecode.push(0);
    }

    let evm_hash = H256(keccak256(&bytecode));

    let padded_bytecode = {
        let mut padded_bytecode: Vec<u8> = vec![];

        let encoded_length = encode(&[Token::Uint(U256::from(bytecode.len()))]);

        padded_bytecode.extend(encoded_length);
        padded_bytecode.extend(bytecode.clone());

        while padded_bytecode.len() % 64 != 32 {
            padded_bytecode.push(0);
        }

        padded_bytecode
    };
    let blob_hash: H256 = hash_evm_bytecode(&padded_bytecode);

    assert!(BlobSha256Format::is_valid(&blob_hash.0));

    // Just some address in user space
    let test_address = Address::from_str(CONTRACT_ADDRESS).unwrap();

    let evm_code_hash_key = get_evm_code_hash_key(&test_address);

    storage.set_value(get_code_key(&test_address), blob_hash);
    storage.set_value(get_known_code_key(&blob_hash), u256_to_h256(U256::one()));

    storage.set_value(evm_code_hash_key, evm_hash);

    storage.store_factory_dep(blob_hash, padded_bytecode);

    let key = storage_key_for_eth_balance(&test_address);
    storage.set_value(key, u256_to_h256(value));

    // Marking bytecode as known

    test_address
}

const TEST_RICH_PK: &str = "0x9e0eee403c6b5963458646fa1b7b3f3c4784138558f9036b0db3435501f2ec6d";
const TEST_RICH_ADDRESS: &str = "0x2140b400689a5dd09c34815958d10affd467f66c";

fn test_evm_vector_with_value(mut bytecode: Vec<u8>, initial_balance: U256, value: U256) -> U256 {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    let test_address =
        insert_evm_contract_with_value(&mut storage, bytecode.clone(), initial_balance);

    // private_key: 0x9e0eee403c6b5963458646fa1b7b3f3c4784138558f9036b0db3435501f2ec6d
    // address: 0x2140b400689a5dd09c34815958d10affd467f66c
    let rich_account: Account = Account::new(H256::from_str(TEST_RICH_PK).unwrap());

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![rich_account.clone()])
        .build();

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(test_address),
            calldata: vec![],
            value: value,
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);

    let debug_tracer = EvmDebugTracer::new();
    let tracer_ptr = debug_tracer.into_tracer_pointer();
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::OneTx);

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
fn test_evm_vector(mut bytecode: Vec<u8>) -> U256 {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    let test_address = insert_evm_contract(&mut storage, bytecode.clone());

    // private_key: 0x9e0eee403c6b5963458646fa1b7b3f3c4784138558f9036b0db3435501f2ec6d
    // address: 0x2140b400689a5dd09c34815958d10affd467f66c
    let rich_account: Account = Account::new(H256::from_str(TEST_RICH_PK).unwrap());

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![rich_account.clone()])
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

    let debug_tracer = EvmDebugTracer::new();
    let tracer_ptr = debug_tracer.into_tracer_pointer();
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::OneTx);

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

//fn test_evm_logs(mut bytecode: Vec<u8>) -> Vec<zksync_types::VmEvent> {
fn test_evm_logs(mut bytecode: Vec<u8>) -> crate::vm_latest::VmExecutionLogs {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    let test_address = insert_evm_contract(&mut storage, bytecode.clone());

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

    //vm.vm.collect_events_and_l1_system_logs_after_timestamp(Timestamp(0)).0
    tx_result.logs
}

fn get_actual_initial_gas() -> U256 {
    let evm_output = test_evm_vector(
        vec![
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    evm_output + 3
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

#[test]
fn test_basic_div_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 6
                hex::decode("7F").unwrap(),
                u256_to_h256(6.into()).0.to_vec(),
                // push32 24
                hex::decode("7F").unwrap(),
                u256_to_h256(24.into()).0.to_vec(),
                // div
                hex::decode("04").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        4.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 3
                hex::decode("7F").unwrap(),
                u256_to_h256(3.into()).0.to_vec(),
                // push32 11
                hex::decode("7F").unwrap(),
                u256_to_h256(11.into()).0.to_vec(),
                // div
                hex::decode("04").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        3.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // push32 4
                hex::decode("7F").unwrap(),
                u256_to_h256(4.into()).0.to_vec(),
                // div
                hex::decode("04").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    )
}

#[test]
fn test_basic_sdiv_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push2 6
                hex::decode("61").unwrap(),
                hex::decode("0006").unwrap(),
                // push2 -4096
                hex::decode("61").unwrap(),
                hex::decode("F000").unwrap(),
                // sdiv
                hex::decode("05").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        10240.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push2 0
                hex::decode("61").unwrap(),
                hex::decode("0000").unwrap(),
                // push2 32
                hex::decode("61").unwrap(),
                hex::decode("0020").unwrap(),
                // sdiv
                hex::decode("05").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_mod_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 5
                hex::decode("60").unwrap(),
                hex::decode("05").unwrap(),
                // push1 18
                hex::decode("60").unwrap(),
                hex::decode("12").unwrap(),
                // mod
                hex::decode("06").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        3.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // push1 7
                hex::decode("60").unwrap(),
                hex::decode("07").unwrap(),
                // mod
                hex::decode("06").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_smod_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 10
                hex::decode("60").unwrap(),
                hex::decode("0A").unwrap(),
                // smod
                hex::decode("07").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
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
                // push2 6
                hex::decode("61").unwrap(),
                hex::decode("0006").unwrap(),
                // push1 -4087
                hex::decode("61").unwrap(),
                hex::decode("F009").unwrap(),
                // smod
                hex::decode("07").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        3.into() // 3
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // push1 14
                hex::decode("60").unwrap(),
                hex::decode("0E").unwrap(),
                // smod
                hex::decode("07").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_addmod_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 8
                hex::decode("7F").unwrap(),
                u256_to_h256(8.into()).0.to_vec(),
                // push32 7
                hex::decode("7F").unwrap(),
                u256_to_h256(7.into()).0.to_vec(),
                // push32 11
                hex::decode("7F").unwrap(),
                u256_to_h256(11.into()).0.to_vec(),
                // addmod
                hex::decode("08").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        2.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 23
                hex::decode("7F").unwrap(),
                u256_to_h256(23.into()).0.to_vec(),
                // push32 42
                hex::decode("7F").unwrap(),
                u256_to_h256(42.into()).0.to_vec(),
                // push32 27
                hex::decode("7F").unwrap(),
                u256_to_h256(27.into()).0.to_vec(),
                // addmod
                hex::decode("08").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_mulmod_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 6
                hex::decode("7F").unwrap(),
                u256_to_h256(6.into()).0.to_vec(),
                // push32 9
                hex::decode("7F").unwrap(),
                u256_to_h256(9.into()).0.to_vec(),
                // push32 15
                hex::decode("7F").unwrap(),
                u256_to_h256(15.into()).0.to_vec(),
                // mulmod
                hex::decode("09").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        3.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 17
                hex::decode("7F").unwrap(),
                u256_to_h256(17.into()).0.to_vec(),
                // push32 24
                hex::decode("7F").unwrap(),
                u256_to_h256(24.into()).0.to_vec(),
                // push32 34
                hex::decode("7F").unwrap(),
                u256_to_h256(34.into()).0.to_vec(),
                // mulmod
                hex::decode("09").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_exp_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 9
                hex::decode("7F").unwrap(),
                u256_to_h256(9.into()).0.to_vec(),
                // push32 5
                hex::decode("7F").unwrap(),
                u256_to_h256(5.into()).0.to_vec(),
                // exp
                hex::decode("0A").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        1_953_125.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // push32 19
                hex::decode("7F").unwrap(),
                u256_to_h256(19.into()).0.to_vec(),
                // exp
                hex::decode("0A").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        1.into()
    );
}

fn perform_exp(base: U256, exponent: U256, result: U256) {
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 base
                hex::decode("7F").unwrap(),
                u256_to_h256(exponent.into()).0.to_vec(),
                // push32 exponent
                hex::decode("7F").unwrap(),
                u256_to_h256(base.into()).0.to_vec(),
                // exp
                hex::decode("0A").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        result.into()
    );
}
#[test]
fn test_multiple_exp_vectors() {
    perform_exp(
        U256::zero(),
        U256::from_dec_str("433478394034343").unwrap(),
        U256::zero(),
    );
    perform_exp(
        U256::one(),
        U256::from_dec_str("433478394034343").unwrap(),
        U256::one(),
    );
    perform_exp(
        U256::from_dec_str("21").unwrap(),
        U256::from_dec_str("52").unwrap(),
        U256::from_dec_str("569381465857367090636427305760163241950353347303833610101782245331441")
            .unwrap(),
    );
    perform_exp(
        U256::max_value(),
        U256::from_dec_str("23784273472384723848213821342323233223").unwrap(),
        U256::from_dec_str(
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        )
        .unwrap(),
    );
}

#[test]
fn test_basic_signextend_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let mut expected_result: [u8; 32] = [0u8; 32];
    hex::decode_to_slice(
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb4da6c",
        &mut expected_result,
    )
    .unwrap();
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 179,624,556
                hex::decode("7F").unwrap(),
                u256_to_h256(179_624_556.into()).0.to_vec(),
                // push32 2
                hex::decode("7F").unwrap(),
                u256_to_h256(2.into()).0.to_vec(),
                // signextend
                hex::decode("0B").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        h256_to_u256(H256::from_slice(&expected_result))
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 179,624,556
                hex::decode("7F").unwrap(),
                u256_to_h256(179_624_556.into()).0.to_vec(),
                // push32 3
                hex::decode("7F").unwrap(),
                u256_to_h256(3.into()).0.to_vec(),
                // signextend
                hex::decode("0B").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        179_624_556.into()
    );
}

#[test]
fn test_basic_keccak_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let evm_vector = test_evm_vector(
        vec![
            // push32 0xFFFF_FFFF
            hex::decode("7F").unwrap(),
            hex::decode("FFFFFFFF00000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 4
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // keccak256
            hex::decode("20").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        H256(evm_vector.into()),
        H256(keccak256(&(0xFFFF_FFFFu32.to_le_bytes())))
    );

    let evm_vector = test_evm_vector(
        vec![
            // push1 4
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // keccak256
            hex::decode("20").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        H256(evm_vector.into()),
        H256(keccak256(&(0x00000_00000u32.to_le_bytes())))
    );
}

#[test]
fn test_basic_keccak_gas() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let consumed_gas_solidity = 57;
    let actual_initial_gas = get_actual_initial_gas();
    let evm_output = test_evm_vector(
        vec![
            // push32 0xFFFF_FFFF
            hex::decode("7F").unwrap(),
            hex::decode("FFFFFFFF00000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 4
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // keccak256
            hex::decode("20").unwrap(),
            //gas
            hex::decode("5A").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let actual_consumed_gas = actual_initial_gas - evm_output;

    println!("Actual consumed gas: {}", actual_consumed_gas);
    assert_eq!(actual_consumed_gas, consumed_gas_solidity.into());
}

#[test]
fn test_basic_dup_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let evm_output = test_evm_vector(
        vec![
            // push32 10
            hex::decode("7F").unwrap(),
            u256_to_h256(10.into()).0.to_vec(),
            // push32 255
            hex::decode("7F").unwrap(),
            u256_to_h256(255.into()).0.to_vec(),
            // push32 100
            hex::decode("7F").unwrap(),
            u256_to_h256(100.into()).0.to_vec(),
            // dup2
            hex::decode("81").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );

    assert_eq!(evm_output, 255.into());

    assert_eq!(
        test_evm_vector(
            vec![
                // push32 179,624,556
                hex::decode("7F").unwrap(),
                u256_to_h256(179_624_556.into()).0.to_vec(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // 16 pushes
                // dup16
                hex::decode("8F").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        179_624_556.into()
    );
}

#[test]
fn test_basic_swap_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let evm_output = test_evm_vector(
        vec![
            // push32 37
            hex::decode("7F").unwrap(),
            u256_to_h256(37.into()).0.to_vec(),
            // push32 255
            hex::decode("7F").unwrap(),
            u256_to_h256(255.into()).0.to_vec(),
            // push32 100
            hex::decode("7F").unwrap(),
            u256_to_h256(100.into()).0.to_vec(),
            // swap2
            //      input output
            // 1     100     37
            // 2     255    255
            // 3      37    100
            hex::decode("91").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 37.into());

    let evm_output = vec![
        // push32 179,624,556
        hex::decode("7F").unwrap(),
        u256_to_h256(179_624_556.into()).0.to_vec(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // 16 pushes
        // push1 10
        hex::decode("60").unwrap(),
        hex::decode("0A").unwrap(),
        // swap16
        hex::decode("9F").unwrap(),
        // push32 0
        hex::decode("7F").unwrap(),
        H256::zero().0.to_vec(),
        // sstore
        hex::decode("55").unwrap(),
    ]
    .into_iter()
    .concat();
    let evm_output = test_evm_vector(evm_output);
    assert_eq!(evm_output, 179_624_556.into());
}

#[test]
fn test_basic_lt_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 8
                hex::decode("60").unwrap(),
                hex::decode("08").unwrap(),
                // push1 10
                hex::decode("60").unwrap(),
                hex::decode("0A").unwrap(),
                // lt
                hex::decode("10").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 24
                hex::decode("60").unwrap(),
                hex::decode("18").unwrap(),
                // push1 10
                hex::decode("60").unwrap(),
                hex::decode("0A").unwrap(),
                // lt
                hex::decode("10").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
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
                // push1 10
                hex::decode("60").unwrap(),
                hex::decode("0A").unwrap(),
                // push1 10
                hex::decode("60").unwrap(),
                hex::decode("0A").unwrap(),
                // lt
                hex::decode("10").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_gt_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 4
                hex::decode("60").unwrap(),
                hex::decode("04").unwrap(),
                // push1 13
                hex::decode("60").unwrap(),
                hex::decode("0D").unwrap(),
                // gt
                hex::decode("11").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
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
                // push1 25
                hex::decode("60").unwrap(),
                hex::decode("19").unwrap(),
                // push1 9
                hex::decode("60").unwrap(),
                hex::decode("09").unwrap(),
                // gt
                hex::decode("11").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 12
                hex::decode("60").unwrap(),
                hex::decode("0C").unwrap(),
                // push1 12
                hex::decode("60").unwrap(),
                hex::decode("0C").unwrap(),
                // gt
                hex::decode("11").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_slt_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 -3
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFD")
                    .unwrap(),
                // push1 13
                hex::decode("60").unwrap(),
                hex::decode("0D").unwrap(),
                // slt
                hex::decode("12").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 7
                hex::decode("60").unwrap(),
                hex::decode("07").unwrap(),
                // push32 -8
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF8")
                    .unwrap(),
                // slt
                hex::decode("12").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
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
                // push1 50
                hex::decode("60").unwrap(),
                hex::decode("3C").unwrap(),
                // push1 50
                hex::decode("60").unwrap(),
                hex::decode("3C").unwrap(),
                // slt
                hex::decode("12").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_sgt_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 -3
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFD")
                    .unwrap(),
                // push1 13
                hex::decode("60").unwrap(),
                hex::decode("0D").unwrap(),
                // sgt
                hex::decode("13").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
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
                // push1 7
                hex::decode("60").unwrap(),
                hex::decode("07").unwrap(),
                // push32 -8
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF8")
                    .unwrap(),
                // sgt
                hex::decode("13").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 50
                hex::decode("60").unwrap(),
                hex::decode("3C").unwrap(),
                // push1 50
                hex::decode("60").unwrap(),
                hex::decode("3C").unwrap(),
                // sgt
                hex::decode("13").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_eq_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 13
                hex::decode("60").unwrap(),
                hex::decode("0D").unwrap(),
                // eq
                hex::decode("14").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 7
                hex::decode("60").unwrap(),
                hex::decode("07").unwrap(),
                // push1 8
                hex::decode("60").unwrap(),
                hex::decode("08").unwrap(),
                // eq
                hex::decode("14").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 50
                hex::decode("60").unwrap(),
                hex::decode("3C").unwrap(),
                // push1 50
                hex::decode("60").unwrap(),
                hex::decode("3C").unwrap(),
                // eq
                hex::decode("14").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        1.into()
    );
}

#[test]
fn test_basic_iszero_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // iszero
                hex::decode("15").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // iszero
                hex::decode("15").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        1.into()
    );
}

#[test]
fn test_basic_xor_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 51
                hex::decode("60").unwrap(),
                hex::decode("33").unwrap(),
                // push1 18
                hex::decode("60").unwrap(),
                hex::decode("12").unwrap(),
                // xor
                hex::decode("18").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        33.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 3
                hex::decode("60").unwrap(),
                hex::decode("03").unwrap(),
                // push1 12
                hex::decode("60").unwrap(),
                hex::decode("0C").unwrap(),
                // xor
                hex::decode("18").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        15.into()
    );
}

#[test]
fn test_basic_not_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 MAX
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                    .unwrap(),
                // not
                hex::decode("19").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFAA")
                    .unwrap(),
                // not
                hex::decode("19").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        85.into()
    );
}

#[test]
fn test_basic_byte_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 31
                hex::decode("60").unwrap(),
                hex::decode("1F").unwrap(),
                // byte
                hex::decode("1A").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        255.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push2 A1FF
                hex::decode("61").unwrap(),
                hex::decode("A1FF").unwrap(),
                // push1 30
                hex::decode("60").unwrap(),
                hex::decode("1E").unwrap(),
                // byte
                hex::decode("1A").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        161.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("B2FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                    .unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // byte
                hex::decode("1A").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        178.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("B2FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                    .unwrap(),
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // byte
                hex::decode("1A").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    // SHL
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // push1 2
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // shl
                hex::decode("1B").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        64.into()
    );
    // SHR
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // push1 2
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // shr
                hex::decode("1C").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        16.into()
    );
    // SAR
    assert_eq!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE")
                    .unwrap(),
                // push1 2
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // sar
                hex::decode("1D").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        U256::from("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
    );
}

#[test]
fn test_basic_jump_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // push1 8
                hex::decode("60").unwrap(),
                hex::decode("08").unwrap(),
                // jump
                hex::decode("56").unwrap(),
                // add
                hex::decode("01").unwrap(),
                // jumpdest
                hex::decode("5B").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        64.into()
    );
}

#[test]
fn test_basic_jumpi_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // push1 1
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // push1 10
                hex::decode("60").unwrap(),
                hex::decode("0A").unwrap(),
                // jumpi
                hex::decode("57").unwrap(),
                // add
                hex::decode("01").unwrap(),
                // jumpdest
                hex::decode("5B").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        64.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // push1 8
                hex::decode("60").unwrap(),
                hex::decode("09").unwrap(),
                // jumpi
                hex::decode("57").unwrap(),
                // add
                hex::decode("01").unwrap(),
                // jumpdest
                hex::decode("5B").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        96.into()
    );
}

#[test]
fn test_basic_block_environment_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.

    // blockhash
    let evm_output = test_evm_vector(
        vec![
            // push32 100
            hex::decode("7F").unwrap(),
            u256_to_h256(100.into()).0.to_vec(),
            // blockhash
            hex::decode("40").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    // Should be 0, since the item in the stack is the blockNumber used
    // The block number 100 was not created, so blockhash(100) == 0.
    assert_eq!(evm_output, 0.into());

    // current block number
    let evm_output = test_evm_vector(
        vec![
            // number
            hex::decode("43").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 1.into());

    // chain-id
    let evm_output = test_evm_vector(
        vec![
            // chain-id
            hex::decode("46").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 270.into());

    // balance
    let evm_output = test_evm_vector(
        vec![
            // push20 TEST_RICH_ADDRESS
            hex::decode("73").unwrap(),
            hex::decode(&TEST_RICH_ADDRESS[2..]).unwrap(), // Remove 0x
            // balance
            hex::decode("31").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        evm_output,
        U256::from_dec_str(
            "115792089237316195423570985008687907853269984665640564039457084007913129639935"
        )
        .unwrap()
    );
}

fn perform_selfbalance(initial_balance: U256, value: U256, result: U256) {
    let evm_output = test_evm_vector_with_value(
        vec![
            // selfbalance
            hex::decode("47").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
        initial_balance,
        value,
    );
    assert_eq!(evm_output, result.into());
}
#[test]
fn test_selfbalance() {
    perform_selfbalance(U256::zero(), U256::zero(), U256::zero());
    perform_selfbalance(U256::one(), U256::zero(), U256::one());
    perform_selfbalance(
        U256::from_dec_str("433478394034343").unwrap(),
        U256::zero(),
        U256::from_dec_str("433478394034343").unwrap(),
    );
    perform_selfbalance(
        U256::from_dec_str("340282366920938463463374607431768211455").unwrap(),
        U256::zero(),
        U256::from_dec_str("340282366920938463463374607431768211455").unwrap(),
    );
    perform_selfbalance(
        U256::from_dec_str("340282366920938463463374607431768211456").unwrap(),
        U256::zero(),
        U256::from_dec_str("340282366920938463463374607431768211456").unwrap(),
    );
    perform_selfbalance(
        U256::from_dec_str("340282366969323363277297521354758540687").unwrap(),
        U256::zero(),
        U256::from_dec_str("340282366969323363277297521354758540687").unwrap(),
    );
    perform_selfbalance(
        U256::from_dec_str("340282366920938463463374607431768211455").unwrap(),
        U256::one(),
        U256::from_dec_str("340282366920938463463374607431768211456").unwrap(),
    );
    perform_selfbalance(
        U256::from_dec_str("340282366920938463463374607431768211455").unwrap(),
        U256::from_dec_str("48384899813922913922990329232").unwrap(),
        U256::from_dec_str("340282366969323363277297521354758540687").unwrap(),
    );
}
#[test]
fn test_basic_pop_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 160
                hex::decode("60").unwrap(),
                hex::decode("A0").unwrap(),
                // push1 31
                hex::decode("60").unwrap(),
                hex::decode("1F").unwrap(),
                // pop
                hex::decode("50").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        160.into()
    );
}

#[test]
fn test_basic_memory_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        255.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push1 2
                hex::decode("60").unwrap(),
                hex::decode("02").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        16_711_680.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 255
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_mstore8_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push2 0xFFFF
                hex::decode("61").unwrap(),
                hex::decode("FFFF").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore8
                hex::decode("53").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        h256_to_u256(
            H256::from_str("FF00000000000000000000000000000000000000000000000000000000000000")
                .unwrap()
        )
        .into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push2 0xAAFF
                hex::decode("61").unwrap(),
                hex::decode("AAFF").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore8
                hex::decode("53").unwrap(),
                // push1 0xBB
                hex::decode("60").unwrap(),
                hex::decode("BB").unwrap(),
                // push1 1
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // mstore8
                hex::decode("53").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        h256_to_u256(
            H256::from_str("FFBB000000000000000000000000000000000000000000000000000000000000")
                .unwrap()
        )
        .into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push2 0xAABB
                hex::decode("61").unwrap(),
                hex::decode("AABB").unwrap(),
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // mstore8
                hex::decode("53").unwrap(),
                // push1 1
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        187.into() // 0xBB
    );
}

#[test]
fn test_basic_sload_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sload
                hex::decode("54").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );

    assert_eq!(
        test_evm_vector(
            vec![
                // push32 2
                hex::decode("7F").unwrap(),
                u256_to_h256(2.into()).0.to_vec(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sstore
                hex::decode("55").unwrap(),
                // push32 0
                hex::decode("7F").unwrap(),
                H256::zero().0.to_vec(),
                // sload
                hex::decode("54").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        2.into()
    );
}

#[test]
#[ignore = "Ignored because gas costs vary constantly as we change the code"]
fn test_sload_gas() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let initial_gas = U256::from_str_radix("1719c754", 16).unwrap();
    let gas_left = test_evm_vector(
        // sload cold
        vec![
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sload
            hex::decode("54").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(initial_gas - gas_left, U256::from_dec_str("2105").unwrap());

    let gas_left_2 = test_evm_vector(
        // sstore cold different value + sload warm
        vec![
            // push32 2
            hex::decode("7F").unwrap(),
            u256_to_h256(2.into()).0.to_vec(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sload
            hex::decode("54").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        initial_gas - gas_left_2,
        U256::from_dec_str("22211").unwrap()
    );

    let gas_left_3 = test_evm_vector(
        // sstore cold same value + sload warm
        vec![
            // push32 0
            hex::decode("7F").unwrap(),
            u256_to_h256(0.into()).0.to_vec(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sload
            hex::decode("54").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        initial_gas - gas_left_3,
        U256::from_dec_str("2311").unwrap()
    );

    let gas_left_4 = test_evm_vector(
        // sstore cold different value + sstore warm same value + sload warm
        vec![
            // push32 2
            hex::decode("7F").unwrap(),
            u256_to_h256(2.into()).0.to_vec(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            u256_to_h256(0.into()).0.to_vec(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sload
            hex::decode("54").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        initial_gas - gas_left_4,
        U256::from_dec_str("22317").unwrap()
    );
}

#[test]
#[ignore = "Ignored because until we define what happens on mstore with expand memory this will fail"]
fn test_basic_msize_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // msize
                hex::decode("59").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );

    assert_eq!(
        test_evm_vector(
            vec![
                // push32 2
                hex::decode("7F").unwrap(),
                u256_to_h256(2.into()).0.to_vec(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // msize
                hex::decode("59").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        32.into()
    );
}

#[test]
#[ignore = "Ignored because until we define what happens on mstore with expand memory this will fail"]
fn test_basic_msize_with_mstore_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push32 2
                hex::decode("7F").unwrap(),
                u256_to_h256(2.into()).0.to_vec(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // msize
                hex::decode("59").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        32.into()
    );
}

#[test]
fn test_basic_caller_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_ne!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // caller
                hex::decode("33").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_callvalue_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 0xFF
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // callvalue
                hex::decode("34").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_calldataload_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // calldataload
                hex::decode("35").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_calldatasize_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 0xDD
                hex::decode("60").unwrap(),
                hex::decode("DD").unwrap(),
                // calldatasize
                hex::decode("36").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_calldatacopy_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 9
                hex::decode("60").unwrap(),
                hex::decode("09").unwrap(),
                // push1 31
                hex::decode("60").unwrap(),
                hex::decode("1F").unwrap(),
                // push1 2
                hex::decode("60").unwrap(),
                hex::decode("02").unwrap(),
                // calldatacopy
                hex::decode("37").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mload
                hex::decode("51").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_calldatacopy_gas() {
    let consumed_gas_solidity = 26;
    let actual_initial_gas = get_actual_initial_gas();
    let evm_output = test_evm_vector(
        vec![
            // push1 9
            hex::decode("60").unwrap(),
            hex::decode("09").unwrap(),
            // push1 31
            hex::decode("60").unwrap(),
            hex::decode("1F").unwrap(),
            // push1 2
            hex::decode("60").unwrap(),
            hex::decode("02").unwrap(),
            // calldatacopy
            hex::decode("37").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let actual_consumed_gas = actual_initial_gas - evm_output;

    assert_eq!(actual_consumed_gas, consumed_gas_solidity.into());
}

#[test]
fn test_basic_address_vectors() {
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 0xFF
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // address
                hex::decode("30").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        h256_to_u256(H256::from(Address::from_str(CONTRACT_ADDRESS).unwrap())).into()
    );
}

#[test]
fn test_basic_balance_vectors() {
    assert_eq!(
        test_evm_vector(
            vec![
                // push20 CONTRACT_ADDRESS
                hex::decode("73").unwrap(),
                hex::decode(&CONTRACT_ADDRESS[2..]).unwrap(), // Remove 0x
                // balance
                hex::decode("31").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    )
}

#[test]
#[ignore = "Ignored because gas costs vary constantly as we change the code"]
fn test_basic_balance_gas_vectors() {
    let initial_gas = U256::from_str_radix("1719c754", 16).unwrap();
    let gas_left = test_evm_vector(
        // Contract address should be warm by default
        vec![
            // push20 CONTRACT_ADDRESS
            hex::decode("73").unwrap(),
            hex::decode(&CONTRACT_ADDRESS[2..]).unwrap(), // Remove 0x
            // balance
            hex::decode("31").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(initial_gas - gas_left, U256::from_dec_str("105").unwrap());

    let gas_left = test_evm_vector(
        // Random Address
        vec![
            // push20 0xab03a0B5963f75f1C8485B355fF6D30f3093BDE8
            hex::decode("73").unwrap(),
            hex::decode("ab03a0B5963f75f1C8485B355fF6D30f3093BDE8").unwrap(),
            // balance
            hex::decode("31").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(initial_gas - gas_left, U256::from_dec_str("2605").unwrap());

    let gas_left = test_evm_vector(
        // Random Address accesed twice
        vec![
            // push20 0xab03a0B5963f75f1C8485B355fF6D30f3093BDE8
            hex::decode("73").unwrap(),
            hex::decode("ab03a0B5963f75f1C8485B355fF6D30f3093BDE8").unwrap(),
            // balance
            hex::decode("31").unwrap(),
            // push20 0xgb03a0B5963f75f1C8485B355fF6D30f3093BDE8
            hex::decode("73").unwrap(),
            hex::decode("ab03a0B5963f75f1C8485B355fF6D30f3093BDE8").unwrap(),
            // balance
            hex::decode("31").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(initial_gas - gas_left, U256::from_dec_str("2708").unwrap());
}

#[test]
fn test_basic_origin_vectors() {
    assert_ne!(
        test_evm_vector(
            vec![
                // push0
                hex::decode("5F").unwrap(),
                // origin
                hex::decode("32").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    )
}

#[test]
fn test_basic_pc_vectors() {
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 0xFF
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // pc
                hex::decode("58").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        2.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // push3 0xFFEEDD
                hex::decode("62").unwrap(),
                hex::decode("FFEEDD").unwrap(),
                // pc
                hex::decode("58").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        4.into()
    );
}

#[test]
#[ignore = "Ignored because gas costs vary constantly as we change the code"]
fn test_basic_gas_vectors() {
    assert_eq!(
        test_evm_vector(
            vec![
                // push1 0xFF
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // gas
                hex::decode("5A").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        U256::from_dec_str("387565391").unwrap().into()
    );

    assert_eq!(
        test_evm_vector(
            vec![
                // push1 0xFF
                hex::decode("60").unwrap(),
                hex::decode("FF").unwrap(),
                // push1 0xEE
                hex::decode("60").unwrap(),
                hex::decode("EE").unwrap(),
                // push1 0xDD
                hex::decode("60").unwrap(),
                hex::decode("DD").unwrap(),
                // push1 0xCC
                hex::decode("60").unwrap(),
                hex::decode("CC").unwrap(),
                // address
                hex::decode("30").unwrap(),
                // gas
                hex::decode("5A").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        U256::from_dec_str("387565380").unwrap().into()
    );
}

#[test]
fn test_basic_create_vectors() {
    assert_ne!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("6080604052348015600e575f80fd5b50603e80601a5f395ff3fe60806040525f")
                    .unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("80fdfea264697066735822122070e77c564e632657f44e4b3cb2d5d4f74255fc")
                    .unwrap(),
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("64ca5fae813eb74275609e61e364736f6c634300081900330000000000000000")
                    .unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push1 88
                hex::decode("60").unwrap(),
                hex::decode("58").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // create
                hex::decode("F0").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_ne!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                    .unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                    .unwrap(),
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                    .unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                    .unwrap(),
                // push1 96
                hex::decode("60").unwrap(),
                hex::decode("60").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                    .unwrap(),
                // push1 128
                hex::decode("60").unwrap(),
                hex::decode("80").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                    .unwrap(),
                // push1 160
                hex::decode("60").unwrap(),
                hex::decode("A0").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                    .unwrap(),
                // push1 192
                hex::decode("60").unwrap(),
                hex::decode("C0").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push1 201
                hex::decode("60").unwrap(),
                hex::decode("C9").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // create
                hex::decode("F0").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
}

#[test]
fn test_basic_create2_vectors() {
    assert_eq!(
        test_evm_vector(
            vec![
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("6080604052348015600e575f80fd5b50603e80601a5f395ff3fe60806040525f")
                    .unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("80fdfea264697066735822122070e77c564e632657f44e4b3cb2d5d4f74255fc")
                    .unwrap(),
                // push1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push32
                hex::decode("7F").unwrap(),
                hex::decode("64ca5fae813eb74275609e61e364736f6c634300081900330000000000000000")
                    .unwrap(),
                // push1 64
                hex::decode("60").unwrap(),
                hex::decode("40").unwrap(),
                // mstore
                hex::decode("52").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // push1 88
                hex::decode("60").unwrap(),
                hex::decode("58").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // create2
                hex::decode("F5").unwrap(),
                // push0
                hex::decode("5F").unwrap(),
                // sstore
                hex::decode("55").unwrap(),
            ]
            .into_iter()
            .concat()
        ),
        h256_to_u256(
            H256::from_str("0x000000000000000000000000f9ce5b3ccbbbe0ce1a33b39bd9c723d048514878")
                .unwrap()
        )
        .into()
    );
}

#[test]
fn test_basic_call_vectors() {
    // Testing with:
    // function decimals() external pure override returns (uint8) {
    //    return 18;
    // }
    // from L2EthToken.sol

    let evm_output = test_evm_vector(
        vec![
            // push4 funcsel
            hex::decode("63").unwrap(),
            hex::decode("313ce567").unwrap(), // func selector
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = funcsel
            // push1 retSize
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // push0 value
            hex::decode("5F").unwrap(),
            // push32 token_contract
            hex::decode("7F").unwrap(),
            hex::decode("000000000000000000000000000000000000000000000000000000000000800A")
                .unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // call
            hex::decode("F1").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 18u32.into());
}

#[test]
fn test_basic_call_with_create_vectors() {
    // The following code is used to test the basic CALL operation
    // if the contract isEVM

    /*
        // Create a contract that creates an exception if first word of calldata is 0
        PUSH17 0x67600035600757FE5B60005260086018F3
        PUSH1 0
        MSTORE
        PUSH1 17
        PUSH1 15
        PUSH1 0
        CREATE

        // Call with non 0 calldata, returns success
        PUSH1 0
        PUSH1 0
        PUSH1 32
        PUSH1 0
        PUSH1 0
        DUP7
        PUSH2 0xFFFF
        CALL
    */

    let evm_output = test_evm_vector(
        vec![
            // push17 bytecode
            // Create a contract that creates an exception if first word of calldata is 0
            hex::decode("70").unwrap(),
            hex::decode("67600035600757FE5B60005260086018F3").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 17
            hex::decode("60").unwrap(),
            hex::decode("11").unwrap(),
            // push1 15
            hex::decode("60").unwrap(),
            hex::decode("0F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // CALL
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // dup6
            hex::decode("85").unwrap(),
            // push2 0xFFFF
            hex::decode("61").unwrap(),
            hex::decode("FFFF").unwrap(),
            // call
            hex::decode("F1").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    // TODO this test is an edge case, taking as example the following code:
    // https://www.evm.codes/playground?fork=shanghai&unit=Wei&codeType=Mnemonic&code='vreate%20ajontracbthatjreates%20a_exceptio_if%20firsbword%20ofjalldata%20isyq17yx67k035k757FE5Bk052k86018F3gzMSTORE~17~15gzCREATEzzvall%20with%20no%20parameters%2C%20returnygg~32ggzDUP6q2yxFFFFzCALL'~q1%20z%5Cny%200v%2F%2F%20CqzPUSHk600j%20cg~0bt%20_n%20%01_bgjkqvyz~_
    //
    // It seems to be an imposed limitation inherited from the implementation, the above code
    // tries to perform a call, since the no return was expected, executes
    // the 0th case of the switch statement of function _saveReturnDataAfterEVMCall()
    //
    // It has to be checked if the error comes from the call operation without parameters
    // or if it is a special case
    // assert_eq!(evm_output, 1u32.into());

    // The following contract is used to test a CALL after a CREATE
    // more rigourously
    /*
        // SPDX-License-Identifier: MIT
        pragma solidity ^0.8.20;

        contract Test {
            // selector == 0x6d4ce63c
            function get() pure public returns (uint256) {
                return 7;
            }
        }
    */

    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // CALL
            //--------------------------
            // push4 funcsel
            hex::decode("63").unwrap(),
            hex::decode("6d4ce63c").unwrap(), // func selector
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = funcsel
            // push1 retSize // 4 byte
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes -> func selector
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff 4bytes
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // push0 value
            hex::decode("5F").unwrap(),
            // dup6 address of created contract
            hex::decode("85").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // call
            hex::decode("F1").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 7u32.into());
}

#[test]
fn test_call_gas() {
    let consumed_gas_solidity = 67636;
    let actual_initial_gas = get_actual_initial_gas();
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // CALL
            //--------------------------
            // push4 funcsel
            hex::decode("63").unwrap(),
            hex::decode("6d4ce63c").unwrap(), // func selector
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = funcsel
            // push1 retSize // 4 byte
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes -> func selector
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff 4bytes
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // push0 value
            hex::decode("5F").unwrap(),
            // dup6 address of created contract
            hex::decode("85").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // call
            hex::decode("F1").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let actual_consumed_gas = actual_initial_gas - evm_output;

    assert_eq!(actual_consumed_gas, consumed_gas_solidity.into());
}

#[test]
fn test_codecopy_gas() {
    let consumed_gas_solidity = 29;
    let actual_initial_gas = get_actual_initial_gas();
    let evm_output = test_evm_vector(
        vec![
            //push0
            hex::decode("5F").unwrap(),
            //push0
            hex::decode("5F").unwrap(),
            //push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // codecopy
            hex::decode("39").unwrap(),
            //push0
            hex::decode("5F").unwrap(),
            //push1 31
            hex::decode("60").unwrap(),
            hex::decode("1F").unwrap(),
            //push1 8
            hex::decode("60").unwrap(),
            hex::decode("08").unwrap(),
            //codecopy
            hex::decode("39").unwrap(),
            // push0
            hex::decode("5f").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let actual_consumed_gas = actual_initial_gas - evm_output;
    println!("actual_consumed_gas: {}", actual_consumed_gas);
    assert_eq!(actual_consumed_gas, consumed_gas_solidity.into());
}

#[test]
fn test_staticcall_gas() {
    let consumed_gas_solidity = 67634;
    let actual_initial_gas = get_actual_initial_gas();
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // STATICCALL
            //--------------------------
            // push4 funcsel
            hex::decode("63").unwrap(),
            hex::decode("6d4ce63c").unwrap(), // func selector
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = funcsel
            // push1 retSize // 4 byte
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes -> func selector
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff 4bytes
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // du5 address of created contract
            hex::decode("84").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // staticcall
            hex::decode("FA").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let actual_consumed_gas = actual_initial_gas - evm_output;
    assert_eq!(actual_consumed_gas, consumed_gas_solidity.into());
}

#[test]
fn test_delegatecall_gas() {
    let consumed_gas_solidity = 67634;
    let actual_initial_gas = get_actual_initial_gas();
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // DELEGATECALL
            //--------------------------
            // push4 funcsel
            hex::decode("63").unwrap(),
            hex::decode("6d4ce63c").unwrap(), // func selector
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = funcsel
            // push1 retSize // 4 byte
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes -> func selector
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff 4bytes
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // du5 address of created contract
            hex::decode("84").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // delegatecall
            hex::decode("F4").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // gas
            hex::decode("5A").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let actual_consumed_gas = actual_initial_gas - evm_output;
    println!("actual_consumed_gas: {}", actual_consumed_gas);
    assert_eq!(actual_consumed_gas, consumed_gas_solidity.into());
}

#[test]
fn test_basic_environment3_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    // gasprice
    let evm_output = test_evm_vector(
        vec![
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 16
            hex::decode("60").unwrap(),
            hex::decode("10").unwrap(),
            // gasprice
            hex::decode("3A").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 250000000.into());

    // test OP_CODECOPY
    let evm_output = test_evm_vector(
        vec![
            // push1 7
            hex::decode("60").unwrap(),
            hex::decode("07").unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // codecopy
            hex::decode("39").unwrap(),
            // push1 0
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        H256(evm_output.into()),
        H256(U256::from("6007600060003900000000000000000000000000000000000000000000000000").into())
    );

    // codesize
    let evm_output = test_evm_vector(
        vec![
            // push1 16
            hex::decode("60").unwrap(), // 1 byte
            hex::decode("10").unwrap(), // 1 byte
            // codesize
            hex::decode("38").unwrap(), // 1 byte
            // push32 0
            hex::decode("7F").unwrap(), // 1 byte
            H256::zero().0.to_vec(),    // 32 bytes
            // sstore
            hex::decode("55").unwrap(), // 1byte
        ]
        .into_iter()
        .concat(),
    );
    // codesize = 37 + memory_expansion = 64 (chunks of 32bytes)
    assert_eq!(evm_output, 64.into());
}

#[test]
fn test_basic_environment4_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    // extcodesize
    // Using the following contract's deployed bytecode (len = 175):
    /*
    // SPDX-License-Identifier: MIT
    pragma solidity ^0.8.20;

    contract Test {
        // selector == 0x6d4ce63c
        function get() pure public returns (uint256) {
            return 7;
        }
    }
    */
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // extcodesize
            hex::decode("3B").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 175.into());

    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("000000000000000000000000000000000000000000000000000000000000800A")
                .unwrap(),
            // extcodesize
            hex::decode("3B").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(evm_output, 8224.into());

    // extcodecopy
    // Creates a constructor that creates a contract with 30 FF and 37 as code
    /*
    PUSH32 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
    PUSH1 0
    MSTORE
    PUSH32 0x3760005260206000F30000000000000000000000000000000000000000000000
    PUSH1 32
    MSTORE

    // Create the contract with the constructor code above
    PUSH1 41
    PUSH1 0
    PUSH1 0
    CREATE // Puts the new contract address on the stack

    // Clear the memory for the examples
    PUSH1 0
    PUSH1 0
    MSTORE
    PUSH1 0
    PUSH1 32
    MSTORE

    // Example 1
    PUSH1 32
    PUSH1 0
    PUSH1 0
    DUP4
    EXTCODECOPY
    */
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3760005260206000F30000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 41
            hex::decode("60").unwrap(),
            hex::decode("29").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // push0 -- clear the memory
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 32 -- begin extcodecopy
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // dup4
            hex::decode("83").unwrap(),
            // extcodecopy
            hex::decode("3C").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        H256(evm_output.into()),
        H256(U256::from("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff37").into())
    );

    let evm_output = test_evm_vector(
        vec![
            // push1 32 -- begin extcodecopy
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("000000000000000000000000000000000000000000000000000000000000800A")
                .unwrap(),
            // extcodecopy
            hex::decode("3C").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(
        H256(evm_output.into()),
        H256(U256::from("0000006003300270000000d6033001970000000102200190000000230000c13d").into())
    );

    // returndatacopy
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // push4 funcsel -- staticcall
            hex::decode("63").unwrap(),
            hex::decode("6d4ce63c").unwrap(), // func selector
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = funcsel
            // push1 retSize // 4 byte
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 32 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes -> func selector
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff 4bytes
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // dup5 address of created contract
            hex::decode("84").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // staticcall
            hex::decode("FA").unwrap(),
            // pop
            hex::decode("50").unwrap(),
            // pop
            hex::decode("50").unwrap(),
            // push1 32 size
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 00 offset
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // push1 64 destOffset
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // returndatacopy
            hex::decode("3E").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    assert_eq!(H256(evm_output.into()), H256(U256::from("7").into()));
}

#[test]
fn test_basic_extcodehash_vectors() {
    // extcodehash
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6080604052348015600e575f80fd5b5060af80601a5f395ff3fe608060405234")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8015600e575f80fd5b50600436106026575f3560e01c80636d4ce63c14602a57")
                .unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("5b5f80fd5b60306044565b604051603b91906062565b60405180910390f35b5f")
                .unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6007905090565b5f819050919050565b605c81604c565b82525050565b5f6020")
                .unwrap(),
            // push1 96
            hex::decode("60").unwrap(),
            hex::decode("60").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("8201905060735f8301846055565b9291505056fea26469706673582212201357")
                .unwrap(),
            // push1 128
            hex::decode("60").unwrap(),
            hex::decode("80").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("3db24498d07df7d6344f02fa1ccf8e15038b10c382a6d71537a002ad4e736473")
                .unwrap(),
            // push1 160
            hex::decode("60").unwrap(),
            hex::decode("A0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("6f6c634300081900330000000000000000000000000000000000000000000000")
                .unwrap(),
            // push1 192
            hex::decode("60").unwrap(),
            hex::decode("C0").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 201
            hex::decode("60").unwrap(),
            hex::decode("C9").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // create
            hex::decode("F0").unwrap(),
            // extcodehash
            hex::decode("3F").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    //assert_eq!(H256(evm_output.into()), H256(U256::from("0").into()));
    println!("{:?}", H256(evm_output.into()));

    // extcodehash
    let evm_output = test_evm_vector(
        vec![
            // push32
            hex::decode("7F").unwrap(),
            hex::decode("0000000000000000000000000000000000000000000000000000000000008002")
                .unwrap(),
            // extcodehash
            hex::decode("3F").unwrap(),
            // push32
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    println!("{:?}", H256(evm_output.into()));
    // bytecodehash obtained from era-contracts: system-contracts/SystemContractsHashes.json
    //assert_eq!(H256(evm_output.into()), H256(U256::from("0100007537b226f7de4103e8c2d1df831e990ff722dc3aca654fd45ce61bd2ec").into()));
}
#[test]
fn test_basic_logs_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    // LOG0
    let evm_output = test_evm_logs(
        vec![
            // push1 37
            hex::decode("60").unwrap(),
            hex::decode("25").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // log0
            hex::decode("A0").unwrap(),
        ]
        .into_iter()
        .concat(),
    );

    for e in evm_output.events {
        if e.address == Address::from_str(CONTRACT_ADDRESS).unwrap() {
            assert!(e.indexed_topics.is_empty());
            assert_eq!(e.value[31], 37u8);
        }
    }

    // LOG1
    let evm_output = test_evm_logs(
        vec![
            // push1 37
            hex::decode("60").unwrap(),
            hex::decode("25").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // log1
            hex::decode("A1").unwrap(),
        ]
        .into_iter()
        .concat(),
    );

    for e in evm_output.events {
        if e.address == Address::from_str(CONTRACT_ADDRESS).unwrap() {
            assert_eq!(e.indexed_topics[0], u256_to_h256(64.into()));
            assert_eq!(e.value[31], 37u8);
        }
    }

    // LOG2
    let evm_output = test_evm_logs(
        vec![
            // push1 37
            hex::decode("60").unwrap(),
            hex::decode("25").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // log2
            hex::decode("A2").unwrap(),
        ]
        .into_iter()
        .concat(),
    );

    for e in evm_output.events {
        if e.address == Address::from_str(CONTRACT_ADDRESS).unwrap() {
            assert_eq!(e.indexed_topics[0], u256_to_h256(32.into()));
            assert_eq!(e.indexed_topics[1], u256_to_h256(64.into()));
            assert_eq!(e.value[31], 37u8);
        }
    }

    // LOG4
    let evm_output = test_evm_logs(
        vec![
            // push1 37
            hex::decode("60").unwrap(),
            hex::decode("25").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 64
            hex::decode("60").unwrap(),
            hex::decode("40").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 16
            hex::decode("60").unwrap(),
            hex::decode("10").unwrap(),
            // push1 12
            hex::decode("60").unwrap(),
            hex::decode("0C").unwrap(),
            // push1 32
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 00
            hex::decode("60").unwrap(),
            hex::decode("00").unwrap(),
            // log4
            hex::decode("A4").unwrap(),
        ]
        .into_iter()
        .concat(),
    );

    for e in evm_output.events {
        if e.address == Address::from_str(CONTRACT_ADDRESS).unwrap() {
            assert_eq!(e.indexed_topics[0], u256_to_h256(12.into()));
            assert_eq!(e.indexed_topics[1], u256_to_h256(16.into()));
            assert_eq!(e.indexed_topics[2], u256_to_h256(32.into()));
            assert_eq!(e.indexed_topics[3], u256_to_h256(64.into()));
            assert_eq!(e.value[31], 37u8);
        }
    }
}

#[test]
#[ignore = "It cannot be tested right now since test_evm_vector makes an assert checking if the transaction fails, In this case we want it to fail"]
fn test_basic_invalid_vectors() {
    // It cannot be tested right now since test_evm_vector makes an assert checking if the transaction fails
    // In this case we want it to fail
    test_evm_vector(
        vec![
            // invalid
            hex::decode("FE").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
}

#[test]
#[ignore = "It cannot be tested right now since we dont yet have a way of checking the results of result opcode"]
fn test_basic_return_vectors() {
    // It cannot be tested right now since we dont yet have a way of checking the results of result opcode
    test_evm_vector(
        vec![
            // push32 0xFF01000000000000000000000000000000000000000000000000000000000000
            hex::decode("7F").unwrap(),
            hex::decode("FF01000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 2
            hex::decode("60").unwrap(),
            hex::decode("02").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // return
            hex::decode("F3").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    // Result should be 0xFF01
}

#[test]
fn test_basic_delegatecall_vectors() {
    assert_eq!(
        test_evm_vector(
            vec![
                // PUSH17 0x67600054600757FE5B60005260086018F3
                hex::decode("70").unwrap(),
                hex::decode("67600054600757FE5B60005260086018F3").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // MSTORE
                hex::decode("52").unwrap(),
                // PUSH1 17
                hex::decode("60").unwrap(),
                hex::decode("11").unwrap(),
                // PUSH1 15
                hex::decode("60").unwrap(),
                hex::decode("0F").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // CREATE
                hex::decode("F0").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // DUP5
                hex::decode("84").unwrap(),
                // PUSH2 0xFFFF
                hex::decode("61").unwrap(),
                hex::decode("FFFF").unwrap(),
                // DELEGATECALL
                hex::decode("F4").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // SSTORE
                hex::decode("55").unwrap()
            ]
            .into_iter()
            .concat()
        ),
        0.into()
    );
    assert_eq!(
        test_evm_vector(
            vec![
                // PUSH17 0x67600054600757FE5B60005260086018F3
                hex::decode("70").unwrap(),
                hex::decode("67600054600757FE5B60005260086018F3").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // MSTORE
                hex::decode("52").unwrap(),
                // PUSH1 17
                hex::decode("60").unwrap(),
                hex::decode("11").unwrap(),
                // PUSH1 15
                hex::decode("60").unwrap(),
                hex::decode("0F").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // CREATE
                hex::decode("F0").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // PUSH1 1
                hex::decode("60").unwrap(),
                hex::decode("01").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // SSTORE
                hex::decode("55").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // PUSH1 32
                hex::decode("60").unwrap(),
                hex::decode("20").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // DUP6
                hex::decode("85").unwrap(),
                // PUSH2 0xFFFF
                hex::decode("71").unwrap(),
                hex::decode("FFFF").unwrap(),
                // DELEGATECALL
                hex::decode("F4").unwrap(),
                // PUSH0
                hex::decode("5F").unwrap(),
                // SSTORE
                hex::decode("55").unwrap()
            ]
            .into_iter()
            .concat()
        ),
        1.into()
    );
}

#[test]
#[ignore = "It cannot be tested right now since test_evm_vector makes an assert checking if the transaction fails, In this case we want it to fail and check if the result is correct"]
fn test_basic_revert_vectors() {
    // It cannot be tested right now since test_evm_vector makes an assert checking if the transaction fails
    // In this case we want it to fail and check if the result is correct
    test_evm_vector(
        vec![
            // push32 0xFF01000000000000000000000000000000000000000000000000000000000000
            hex::decode("7F").unwrap(),
            hex::decode("FF01000000000000000000000000000000000000000000000000000000000000")
                .unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // push1 2
            hex::decode("60").unwrap(),
            hex::decode("02").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // revert
            hex::decode("FD").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    // Result should be 0xFF01, maybe it includes the gas before?
}

#[test]
fn test_basic_precompiles_vectors() {
    let evm_vector = test_evm_vector(
        vec![
            // push4 FFFF_FFFF
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = FFFF_FFFF
            // push1 retSize
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 4bytes
            hex::decode("60").unwrap(),
            hex::decode("04").unwrap(),
            // push1 argOff
            hex::decode("60").unwrap(),
            hex::decode("1C").unwrap(),
            // push1 0x2 -- SHA256 precompile
            hex::decode("60").unwrap(),
            hex::decode("02").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // staticcall
            hex::decode("FA").unwrap(),
            // pop
            hex::decode("50").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    // calldata for `SHA256(FFFFFFFF)`.
    let sha256 = "ad95131bc0b799c0b1af477fb14fcf26a6a9f76079e48bf090acb7e8367bfd0e";
    assert_eq!(H256(evm_vector.into()), H256::from_str(sha256).unwrap());

    let evm_vector = test_evm_vector(
        vec![
            // push1
            hex::decode("60").unwrap(),
            hex::decode("FF").unwrap(),
            // push0
            hex::decode("5F").unwrap(),
            // mstore
            hex::decode("52").unwrap(),
            // mem[0] = FF
            // push1 retSize
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 retOff
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // push1 argSize 1bytes
            hex::decode("60").unwrap(),
            hex::decode("01").unwrap(),
            // push1 argOff
            hex::decode("60").unwrap(),
            hex::decode("1F").unwrap(),
            // push0 value
            hex::decode("5F").unwrap(),
            // push1 0x2 -- SHA256 precompile
            hex::decode("60").unwrap(),
            hex::decode("02").unwrap(),
            // push4 gas
            hex::decode("63").unwrap(),
            hex::decode("FFFFFFFF").unwrap(),
            // call
            hex::decode("F1").unwrap(),
            // pop
            hex::decode("50").unwrap(),
            // push1 memOffset
            hex::decode("60").unwrap(),
            hex::decode("20").unwrap(),
            // mload
            hex::decode("51").unwrap(),
            // push32 0
            hex::decode("7F").unwrap(),
            H256::zero().0.to_vec(),
            // sstore
            hex::decode("55").unwrap(),
        ]
        .into_iter()
        .concat(),
    );
    let sha256 = "a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89";
    assert_eq!(H256(evm_vector.into()), H256::from_str(sha256).unwrap());
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

// fn deploy_evm_contrac2<H: HistoryMode>(
//     dispatcher: TracerDispatcher<StorageView<InMemoryStorage>, HistoryEnabled>,
//     tester: &mut VmTester<H>,
//     folder_name: &str,
//     contract_name: &str,
// ) -> (Address, Contract) {
//     let account = &mut tester.rich_accounts[0];

//     let (counter_bytecode, counter_deployed_bytecode) =
//         read_test_evm_bytecode(folder_name, contract_name);
//     let abi = load_test_evm_contract(folder_name, contract_name);

//     let sample_evm_code = counter_bytecode;
//     let expected_deployed_code_hash = H256(keccak256(&counter_deployed_bytecode));

//     let tx = account.get_l2_tx_for_execute(
//         Execute {
//             contract_address: None,
//             calldata: sample_evm_code,
//             value: U256::zero(),
//             factory_deps: None,
//         },
//         None,
//     );

//     tester.vm.push_transaction(tx);
//     let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
//         tester.vm.inspect(dispatcher.into(), VmExecutionMode::OneTx);

//     assert!(
//         !tx_result.result.is_failed(),
//         "Transaction wasn't successful"
//     );

//     let expected_deployed_address = deployed_address_evm_create(account.address, U256::zero());
//     assert_deployed_hash(
//         tester,
//         expected_deployed_address,
//         expected_deployed_code_hash,
//     );

//     (expected_deployed_address, abi)
// }

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
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        tester.vm.execute(VmExecutionMode::OneTx);

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
        EvmDebugTracer::new().into_tracer_pointer().into(),
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
        // One, because newly deployed EVM contract will have nonce 1
        deployed_address_evm_create(factory_address, U256::one()),
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

#[test]
fn test_evm_staticcall_behavior() {
    let zkevm_static_caller = read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/evm-simulator/StaticCaller.sol/StaticCallTester.json");
    let zkevm_static_caller_abi = load_contract("etc/contracts-test-data/artifacts-zk/contracts/evm-simulator/StaticCaller.sol/StaticCallTester.json");
    let zkevm_static_caller_address = Address::random();

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .with_custom_contracts(vec![(
            zkevm_static_caller,
            zkevm_static_caller_address,
            false,
        )])
        .build();

    let (address, abi) = deploy_evm_contract(&mut vm, "staticcall", "StaticCallTester");
    let account = &mut vm.rich_accounts[0];

    // Firsly, we check the correct behavior within EVM only.
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(address),
            calldata: abi.function("test").unwrap().encode_input(&[]).unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);
    println!("{:#?}", tx_result.result);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    // Secondly, we check the correct behavior when zkEVM calls EVM.
    let test_inner_calldata = abi
        .function("testInner")
        .unwrap()
        .encode_input(&[])
        .unwrap();
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(zkevm_static_caller_address),
            calldata: zkevm_static_caller_abi
                .function("performStaticCall")
                .unwrap()
                .encode_input(&[Token::Address(address), Token::Bytes(test_inner_calldata)])
                .unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );
    vm.vm.push_transaction(tx);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);
    println!("{:#?}", tx_result.result);
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let batch_result = vm.vm.execute(VmExecutionMode::Batch);
    assert!(!batch_result.result.is_failed(), "Batch wasn't successful");
}

struct EVMOpcodeBenchmarkParams {
    pub number_of_opcodes: usize,
    pub filler: Vec<u8>,
    pub opcode: u8,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct EVMOpcodeBenchmarkResult {
    pub used_zkevm_ergs: u32,
    pub used_evm_gas: u32,
    pub used_circuits: f32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct EVMOpcodeBenchmarkResultWithName {
    pub name: String,
    pub used_zkevm_ergs: u32,
    pub used_evm_gas: u32,
    pub used_circuits: f32,
}

#[derive(Debug, Default)]
struct ZkEVMBenchmarkResult {
    pub used_zkevm_ergs: u32,
    pub used_circuits: f32,
}

impl Sub for EVMOpcodeBenchmarkResult {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        EVMOpcodeBenchmarkResult {
            used_zkevm_ergs: self.used_zkevm_ergs - other.used_zkevm_ergs,
            used_evm_gas: self.used_evm_gas - other.used_evm_gas,
            used_circuits: self.used_circuits - other.used_circuits,
        }
    }
}

impl Div<usize> for EVMOpcodeBenchmarkResult {
    type Output = Self;

    fn div(self, other: usize) -> Self {
        EVMOpcodeBenchmarkResult {
            used_zkevm_ergs: self.used_zkevm_ergs / other as u32,
            used_evm_gas: self.used_evm_gas / other as u32,
            used_circuits: self.used_circuits / other as f32,
        }
    }
}

fn encode_multiple_push32(values: Vec<U256>) -> Vec<u8> {
    values
        .into_iter()
        .flat_map(|value| {
            let mut result: Vec<u8> = vec![0x7f];
            result.extend_from_slice(&u256_to_h256(value).0);
            result
        })
        .collect()
}

// eth transfer is very similar by cost to ERC20 transfer in zkEVM. A bit smaller, but gives a good refernce point
fn perform_zkevm_benchmark() -> ZkEVMBenchmarkResult {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    // The first transaction's result can be pollutted with ergs used by the initial bootloader preparations, so we conduct one tx before conducting a
    // the benchmarking transaction.

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(Address::zero()),
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

    // Now, we can do the benchmarking transaction.

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(Address::zero()),
            calldata: vec![],
            value: U256::one(),
            factory_deps: None,
        },
        None,
    );
    let ergs_before = vm.vm.gas_remaining();
    vm.vm.push_transaction(tx);
    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);
    let ergs_after = vm.vm.gas_remaining();
    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    ZkEVMBenchmarkResult {
        used_zkevm_ergs: ergs_before - ergs_after,
        used_circuits: tx_result.statistics.circuit_statistic.total_f32(),
    }
}

fn perform_benchmark(bytecode: Vec<u8>) -> EVMOpcodeBenchmarkResult {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    let test_address = insert_evm_contract(&mut storage, bytecode.clone());

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let (benchmark_address, abi) = deploy_evm_contract(&mut vm, "benchmark", "BenchmarkCaller");

    let account = &mut vm.rich_accounts[0];

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(benchmark_address),
            calldata: abi
                .function("callAndBenchmark")
                .unwrap()
                .encode_input(&[Token::Address(test_address)])
                .unwrap(),
            value: U256::zero(),
            factory_deps: None,
        },
        None,
    );

    vm.vm.push_transaction(tx);

    let ergs_before = vm.vm.gas_remaining();

    let tx_result: crate::vm_latest::VmExecutionResultAndLogs =
        vm.vm.execute(VmExecutionMode::OneTx);

    let ergs_after = vm.vm.gas_remaining();

    assert!(
        !tx_result.result.is_failed(),
        "Transaction wasn't successful"
    );

    let used_evm_gas = vm.vm.storage.borrow_mut().get_value(&StorageKey::new(
        AccountTreeId::new(benchmark_address),
        H256::zero(),
    ));

    EVMOpcodeBenchmarkResult {
        used_zkevm_ergs: ergs_before - ergs_after,
        used_evm_gas: h256_to_u256(used_evm_gas).as_u32(),
        used_circuits: tx_result.statistics.circuit_statistic.total_f32(),
    }
}

fn perform_opcode_benchmark(params: EVMOpcodeBenchmarkParams) -> EVMOpcodeBenchmarkResult {
    /*
        The test works the following way:

        Let’s say that an opcode is like `ADD`` and it takes `N` params and we want to execute it `K` times.

        We have to somehow extract the price of individual opcode, i.e. ensure that no other actions (such as copying the bytecode) distort the results.

        We’ll need `N * K` params. So we’ll need `N * K` PUSH32 operations first. And the overall length of the bytecode will be `LEN = N * K + K` to accommodate for the opcode itself. So the algorithm will be the following one:

        1. Create a contract with bytecode `LEN` bytes long and the corresponding `N * K` PUSH32 operations (the rest `K` bytes are zeroes). The bytecode will be full of 0s. Run the benchmark. It will return the number of ergs needed to process such bytecode without the tested opcode.
        2. Create a contract with bytecode `LEN` bytes long and the corresponding `N * K` PUSH32 operations, where after each `N` operations there will be one of the tested opcode. It will return the number of ergs needed to process such bytecode with the tested opcode.
    */

    let bytecode_len = params.number_of_opcodes * params.filler.len() + params.number_of_opcodes;

    let mut bytecode_with_filler_only = vec![0u8; bytecode_len];
    for i in 0..params.number_of_opcodes {
        let start = i * params.filler.len();
        let end = start + params.filler.len();
        bytecode_with_filler_only[start..end].copy_from_slice(&params.filler);
    }

    let mut bytecode_with_filler_and_opcode = vec![0u8; bytecode_len];
    for i in 0..params.number_of_opcodes {
        let start = i * params.filler.len() + i;
        let end = start + params.filler.len();
        bytecode_with_filler_and_opcode[start..end].copy_from_slice(&params.filler);
        bytecode_with_filler_and_opcode[end] = params.opcode;
    }

    let benchmark_with_filler_only = perform_benchmark(bytecode_with_filler_only);
    let benchmark_with_filler_and_opcode = perform_benchmark(bytecode_with_filler_and_opcode);

    let diff = benchmark_with_filler_and_opcode - benchmark_with_filler_only;

    diff / params.number_of_opcodes
}

fn start_benchmark() -> Result<String, Box<dyn std::error::Error>> {
    let evm_version = env::var("EVM_SIMULATOR").unwrap_or_else(|_| "yul".to_string());
    let now = Utc::now();
    let year = now.year();
    let month = now.month();
    let day = now.day();
    let hour = now.hour();
    let minute = now.minute();
    let second = now.second();
    let formatted_time = format!(
        "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}",
        year, month, day, hour, minute, second
    );
    let directory_path = format!("benchmarks");
    if !std::fs::metadata(&directory_path).is_ok() {
        // If it doesn't exist, create it
        std::fs::create_dir(&directory_path)?;
    }
    let filename = format!(
        "benchmarks/benchmark_{}_{}.csv",
        evm_version, formatted_time
    );

    Ok(filename.to_string())
}
fn save_benchmark(
    name: &str,
    filename: &String,
    result: EVMOpcodeBenchmarkResult,
) -> Result<(), Box<dyn std::error::Error>> {
    let result_with_name = EVMOpcodeBenchmarkResultWithName {
        name: name.to_string(),
        used_zkevm_ergs: result.used_zkevm_ergs,
        used_evm_gas: result.used_evm_gas,
        used_circuits: result.used_circuits,
    };
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(filename)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Move the cursor to the start of the file
    file.seek(SeekFrom::Start(0))?;

    // Deserialize existing CSV into a vector of EVMOpcodeBenchmarkResult
    let mut results: Vec<EVMOpcodeBenchmarkResultWithName> = if contents.is_empty() {
        Vec::new()
    } else {
        let mut reader = ReaderBuilder::new().from_reader(contents.as_bytes());
        reader
            .deserialize::<EVMOpcodeBenchmarkResultWithName>()
            .collect::<Result<Vec<_>, _>>()?
    };

    // Push the new result
    results.push(result_with_name);

    // Serialize the vector back to CSV
    let mut writer = WriterBuilder::new().from_writer(file);

    for result in &results {
        writer.serialize(result)?;
    }

    writer.flush()?;

    Ok(())
}

fn perform_benchmark_and_save(
    name: &str,
    filename: &String,
    bytecode: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = perform_benchmark(bytecode);
    save_benchmark(name, filename, result)
}

fn benchmark_basic(filename: &String) {
    let name = "benchmark_basic";
    let bytecode = vec![
        // push1 0
        hex::decode("60").unwrap(),
        hex::decode("00").unwrap(),
        // push1 1
        hex::decode("60").unwrap(),
        hex::decode("01").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 2
        hex::decode("60").unwrap(),
        hex::decode("02").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 4
        hex::decode("60").unwrap(),
        hex::decode("04").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 5
        hex::decode("60").unwrap(),
        hex::decode("05").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 6
        hex::decode("60").unwrap(),
        hex::decode("06").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 7
        hex::decode("60").unwrap(),
        hex::decode("07").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 8
        hex::decode("60").unwrap(),
        hex::decode("08").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 9
        hex::decode("60").unwrap(),
        hex::decode("09").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 10
        hex::decode("60").unwrap(),
        hex::decode("0A").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 11
        hex::decode("60").unwrap(),
        hex::decode("0B").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 12
        hex::decode("60").unwrap(),
        hex::decode("0C").unwrap(),
        // add
        hex::decode("01").unwrap(),
        // push1 13
        hex::decode("60").unwrap(),
    ]
    .into_iter()
    .concat();

    perform_benchmark_and_save(name, filename, bytecode).unwrap();
}

fn benchmark_basic2(filename: &String) {
    let name = "benchmark_basic_2";
    let bytecode = vec![
        // push32 179,624,556
        hex::decode("7F").unwrap(),
        u256_to_h256(179_624_556.into()).0.to_vec(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // 16 pushes
        // dup16
        hex::decode("8F").unwrap(),
        // push32 0
        hex::decode("7F").unwrap(),
        H256::zero().0.to_vec(),
        // sstore
        hex::decode("55").unwrap(),
    ]
    .into_iter()
    .concat();

    perform_benchmark_and_save(name, filename, bytecode).unwrap();
}

fn benchmark_basic_3(filename: &String) {
    let name = "benchmark_basic_3";
    let bytecode = vec![
        // push32 179,624,556
        hex::decode("7F").unwrap(),
        u256_to_h256(179_624_556.into()).0.to_vec(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // push1 3
        hex::decode("60").unwrap(),
        hex::decode("03").unwrap(),
        // push1 255
        hex::decode("60").unwrap(),
        hex::decode("FF").unwrap(),
        // 16 pushes
        // push1 10
        hex::decode("60").unwrap(),
        hex::decode("0A").unwrap(),
        // swap16
        hex::decode("9F").unwrap(),
        // push32 0
        hex::decode("7F").unwrap(),
        H256::zero().0.to_vec(),
        // sstore
        hex::decode("55").unwrap(),
    ]
    .into_iter()
    .concat();

    perform_benchmark_and_save(name, filename, bytecode).unwrap();
}
// TODO: move this test to a separate binary
#[test]
fn test_evm_benchmark() {
    let filename = start_benchmark().unwrap();
    benchmark_basic(&filename);
    benchmark_basic2(&filename);
    benchmark_basic_3(&filename);
}
