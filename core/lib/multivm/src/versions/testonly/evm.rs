use std::iter;

use assert_matches::assert_matches;
use ethabi::Token;
use zksync_contracts::{load_contract, read_deployed_bytecode_from_path};
use zksync_test_account::Account;
use zksync_types::{
    block::L2BlockHasher, get_code_key, get_deployer_key, get_evm_code_hash_key,
    get_known_code_key, system_contracts::get_system_smart_contracts, web3, Address, Execute,
    L2BlockNumber, ProtocolVersionId, H256, U256,
};
use zksync_utils::bytecode::{hash_bytecode, hash_evm_bytecode};

use crate::{
    interface::{
        storage::InMemoryStorage, ExecutionResult, L2BlockEnv, TxExecutionMode, VmInterfaceExt,
        VmRevertReason,
    },
    utils::get_batch_base_fee,
    versions::testonly::{default_system_env, TestedVm, VmTester, VmTesterBuilder},
};

const EVM_TEST_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts/evm.sol/EvmEmulationTest.json";

const EVM_ADDRESS: Address = Address::repeat_byte(1);

fn pad_evm_bytecode(deployed_bytecode: &[u8]) -> Vec<u8> {
    let mut padded = Vec::with_capacity(deployed_bytecode.len() + 32);
    let len = U256::from(deployed_bytecode.len());
    padded.extend_from_slice(&[0; 32]);
    len.to_big_endian(&mut padded);
    padded.extend_from_slice(deployed_bytecode);

    // Pad to the 32-byte word boundary.
    if padded.len() % 32 != 0 {
        padded.extend(iter::repeat(0).take(32 - padded.len() % 32));
    }
    assert_eq!(padded.len() % 32, 0);

    // Pad to contain the odd number of words.
    if (padded.len() / 32) % 2 != 1 {
        padded.extend_from_slice(&[0; 32]);
    }
    assert_eq!((padded.len() / 32) % 2, 1);
    padded
}

fn prepare_tester_with_real_emulator<VM: TestedVm>() -> (VmTester<VM>, H256) {
    let deployed_evm_bytecode =
        read_deployed_bytecode_from_path(EVM_TEST_CONTRACT_PATH.as_ref()).unwrap();
    let evm_bytecode_keccak_hash = H256(web3::keccak256(&deployed_evm_bytecode));
    let deployed_evm_bytecode = pad_evm_bytecode(&deployed_evm_bytecode);
    let evm_bytecode_hash = hash_evm_bytecode(&deployed_evm_bytecode);

    let mut system_env = default_system_env();
    system_env.base_system_smart_contracts = system_env
        .base_system_smart_contracts
        .with_latest_evm_emulator();
    let mut storage = InMemoryStorage::with_custom_system_contracts_and_chain_id(
        system_env.chain_id,
        hash_bytecode,
        get_system_smart_contracts(true),
    );
    let evm_emulator_hash = system_env
        .base_system_smart_contracts
        .hashes()
        .evm_emulator
        .unwrap();
    storage.set_value(
        get_deployer_key(H256::from_low_u64_be(1)),
        evm_emulator_hash,
    );
    // Mark the EVM contract as deployed.
    storage.set_value(
        get_known_code_key(&evm_bytecode_hash),
        H256::from_low_u64_be(1),
    );
    storage.set_value(get_code_key(&EVM_ADDRESS), evm_bytecode_hash);
    storage.set_value(
        get_evm_code_hash_key(&EVM_ADDRESS),
        evm_bytecode_keccak_hash,
    );
    storage.store_factory_dep(evm_bytecode_hash, deployed_evm_bytecode);

    let tester = VmTesterBuilder::new()
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    (tester, evm_bytecode_keccak_hash)
}

pub(crate) fn test_real_emulator_basics<VM: TestedVm>() {
    let (mut vm, _) = prepare_tester_with_real_emulator::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let test_fn = evm_abi.function("testCall").unwrap();

    let success_call = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Bool(false)]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = account.get_l2_tx_for_execute(success_call, None);

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert!(new_known_factory_deps.is_empty());

    let reverting_call = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Bool(true)]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = account.get_l2_tx_for_execute(reverting_call, None);

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);

    assert_matches!(
        &vm_result.result,
        ExecutionResult::Revert { output: VmRevertReason::General { msg, .. } }
            if msg == "requested revert"
    );
}

pub(crate) fn test_real_emulator_code_hash<VM: TestedVm>() {
    let (mut vm, expected_hash) = prepare_tester_with_real_emulator::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let test_fn = evm_abi.function("testCodeHash").unwrap();

    let test_execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn
            .encode_input(&[Token::FixedBytes(expected_hash.0.to_vec())])
            .unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = account.get_l2_tx_for_execute(test_execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

fn create_test_block_execute(abi: &ethabi::Contract, block: &L2BlockEnv) -> Execute {
    let test_fn = abi.function("testBlockInfo").unwrap();
    Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn
            .encode_input(&[
                Token::Uint(block.number.into()),
                Token::Uint(block.timestamp.into()),
                Token::FixedBytes(block.prev_block_hash.0.to_vec()),
            ])
            .unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    }
}

pub(crate) fn test_real_emulator_block_info<VM: TestedVm>() {
    let (mut vm, _) = prepare_tester_with_real_emulator::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);

    let first_block = vm.l1_batch_env.first_l2_block;
    let tx = account.get_l2_tx_for_execute(create_test_block_execute(&evm_abi, &first_block), None);
    let tx_hash = tx.hash();
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let mut block_hasher = L2BlockHasher::new(
        L2BlockNumber(first_block.number),
        first_block.timestamp,
        first_block.prev_block_hash,
    );
    block_hasher.push_tx_hash(tx_hash);
    let second_block = L2BlockEnv {
        number: first_block.number + 1,
        timestamp: first_block.timestamp + 5,
        prev_block_hash: block_hasher.finalize(ProtocolVersionId::latest()),
        max_virtual_blocks_to_create: 1,
    };
    vm.vm.start_new_l2_block(second_block);

    let tx =
        account.get_l2_tx_for_execute(create_test_block_execute(&evm_abi, &second_block), None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

pub(crate) fn test_real_emulator_msg_info<VM: TestedVm>() {
    let (mut vm, _) = prepare_tester_with_real_emulator::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);

    let test_fn = evm_abi.function("testMsgInfo").unwrap();
    let data = (0..42).collect();
    let execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Bytes(data)]).unwrap(),
        value: U256::from(10).pow(18.into()), // 1 ether; asserted in the test fn
        factory_deps: vec![],
    };
    let fee = Account::default_fee();
    let base_fee = get_batch_base_fee(&vm.l1_batch_env, vm.system_env.version.into());
    assert_eq!(base_fee, 250_000_000); // asserted in the test fn

    let tx = account.get_l2_tx_for_execute(execute, Some(fee));
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

// FIXME: stipend overflow for far call recursion
pub(crate) fn test_real_emulator_recursion<VM: TestedVm>() {
    let (mut vm, _) = prepare_tester_with_real_emulator::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let test_fn = evm_abi.function("testRecursion").unwrap();

    for use_far_calls in [false, true] {
        println!("use_far_calls = {use_far_calls:?}");
        let test_execute = Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata: test_fn.encode_input(&[Token::Bool(use_far_calls)]).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        };
        let tx = account.get_l2_tx_for_execute(test_execute, None);
        let (_, vm_result) = vm
            .vm
            .execute_transaction_with_bytecode_compression(tx, true);
        assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
        let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
        assert!(new_known_factory_deps.is_empty());
    }
}
