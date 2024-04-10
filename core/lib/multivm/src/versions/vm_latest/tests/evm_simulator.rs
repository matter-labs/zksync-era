use std::{
    ops::{Div, Sub},
    str::FromStr,
};

use ethabi::{encode, ethereum_types::H264, Contract, Token};
use itertools::Itertools;
use tracing::{instrument::WithSubscriber, Instrument};
// FIXME: 1.4.1 should not be imported from 1.5.0
use zk_evm_1_4_1::sha2::{self};
use zk_evm_1_5_0::zkevm_opcode_defs::{BlobSha256Format, VersionedHashLen32};
use zksync_contracts::{load_contract, read_bytecode, read_evm_bytecode};
use zksync_state::{InMemoryStorage, StorageView};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    get_address_mapping_key, get_code_key, get_deployer_key, get_evm_code_hash_key,
    get_known_code_key,
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
    vm_boojum_integration::tracers::dispatcher,
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
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
    let test_address = Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap();

    let evm_code_hash_key = get_evm_code_hash_key(&test_address);

    storage.set_value(get_code_key(&test_address), blob_hash);
    storage.set_value(get_known_code_key(&blob_hash), u256_to_h256(U256::one()));

    storage.set_value(evm_code_hash_key, evm_hash);

    storage.store_factory_dep(blob_hash, padded_bytecode);

    // Marking bytecode as known

    test_address
}

fn test_evm_vector(mut bytecode: Vec<u8>) -> U256 {
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

    let debug_tracer = EvmDebugTracer::new();
    let tracer_ptr = debug_tracer.into_tracer_pointer();
    let tx_result = vm.vm.inspect(tracer_ptr.into(), VmExecutionMode::OneTx);

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
fn test_basic_sload_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    assert_eq!(
        test_evm_vector( // TODO: fix this tests
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
fn test_sload_gas() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    let initial_gas = U256::MAX;
    let gas_left = test_evm_vector( // sload cold
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
            .concat()
        );
    assert_eq!(initial_gas - gas_left,U256::from_dec_str("2105").unwrap());

    let gas_left_2 = 
        test_evm_vector( // sstore cold different value + sload warm
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
            .concat()
        );
    assert_eq!(initial_gas - gas_left_2,U256::from_dec_str("22211").unwrap());

    let gas_left_3 = 
        test_evm_vector( // sstore cold same value + sload warm
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
            .concat()
        );
    assert_eq!(initial_gas - gas_left_3,U256::from_dec_str("2311").unwrap());

    let gas_left_4 = 
        test_evm_vector( // sstore cold different value + sstore warm same value + sload warm
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
            .concat()
        );
    assert_eq!(initial_gas - gas_left_4,U256::from_dec_str("22317").unwrap());
}

#[test]
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
    // TODO: When mstore (with memory expansion) is implemented, uncomment this test
    /* assert_eq!(
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
    );*/ 
}

#[test]
fn test_basic_msize_with_mstore_vectors() {
    // Here we just try to test some small EVM contracts and ensure that they work.
    // TODO: rigth now it should fail, when mstore (with memory expansion) is implemented, this should work
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
fn test_basic_gas_vectors() {
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
        h256_to_u256(H256::from(
            Address::from_str("0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7").unwrap()
        ))
        .into()
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

#[derive(Debug, Default)]
struct EVMOpcodeBenchmarkResult {
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

// TODO: move this test to a separate binary
#[test]
fn test_evm_benchmark() {
    println!("{:#?}", perform_zkevm_benchmark());

    println!(
        "{:#?}",
        perform_opcode_benchmark(EVMOpcodeBenchmarkParams {
            number_of_opcodes: 50,
            filler: encode_multiple_push32(vec![
                U256::from(2).pow(255.into()) + U256::from(1),
                U256::from(2).pow(255.into())
            ]),
            opcode: 1 // add
        })
    );
}
