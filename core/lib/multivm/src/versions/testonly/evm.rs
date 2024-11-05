use std::{collections::HashMap, iter};

use assert_matches::assert_matches;
use ethabi::Token;
use rand::{rngs::StdRng, Rng, SeedableRng};
use zksync_contracts::{load_contract, read_deployed_bytecode_from_path};
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher, get_code_key, get_deployer_key, get_evm_code_hash_key,
    get_known_code_key, system_contracts::get_system_smart_contracts, web3, AccountTreeId, Address,
    Execute, L2BlockNumber, ProtocolVersionId, StorageKey, H256, U256,
};
use zksync_utils::{
    address_to_h256,
    bytecode::{hash_bytecode, hash_evm_bytecode},
    h256_to_account_address, h256_to_u256,
};

use super::{
    default_system_env, load_test_contract_abi, read_proxy_counter_contract, read_test_contract,
    ContractToDeploy, TestedVm, VmTester, VmTesterBuilder,
};
use crate::{
    interface::{
        storage::InMemoryStorage, ExecutionResult, L2BlockEnv, TxExecutionMode, VmInterfaceExt,
        VmRevertReason,
    },
    utils::get_batch_base_fee,
};

const EVM_TEST_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts/evm.sol/EvmEmulationTest.json";
const EVM_COUNTER_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts/evm.sol/Counter.json";

const EVM_ADDRESS: Address = Address::repeat_byte(1);
const ERAVM_COUNTER_ADDRESS: Address = Address::repeat_byte(2);

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

fn prepare_tester_with_real_emulator() -> (VmTesterBuilder, Vec<u8>) {
    let deployed_evm_bytecode =
        read_deployed_bytecode_from_path(EVM_TEST_CONTRACT_PATH.as_ref()).unwrap();
    let evm_bytecode_keccak_hash = H256(web3::keccak256(&deployed_evm_bytecode));
    let padded_evm_bytecode = pad_evm_bytecode(&deployed_evm_bytecode);
    let evm_bytecode_hash = hash_evm_bytecode(&padded_evm_bytecode);

    let mut system_env = default_system_env();
    system_env.base_system_smart_contracts = system_env
        .base_system_smart_contracts
        .with_latest_evm_emulator();
    let mut storage = InMemoryStorage::with_custom_system_contracts_and_chain_id(
        system_env.chain_id,
        hash_bytecode,
        get_system_smart_contracts(true),
    );
    // Set `ALLOWED_BYTECODE_TYPES_MODE_SLOT` in `ContractDeployer`.
    storage.set_value(
        get_deployer_key(H256::from_low_u64_be(2)),
        H256::from_low_u64_be(1),
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
    storage.store_factory_dep(evm_bytecode_hash, padded_evm_bytecode);

    let tester_builder = VmTesterBuilder::new()
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1);
    (tester_builder, deployed_evm_bytecode)
}

pub(crate) fn test_real_emulator_basics<VM: TestedVm>() {
    let eravm_counter_bytecode = read_test_contract();
    let mut vm = prepare_tester_with_real_emulator()
        .0
        .with_custom_contracts(vec![ContractToDeploy::new(
            eravm_counter_bytecode,
            ERAVM_COUNTER_ADDRESS,
        )])
        .build::<VM>();
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);

    call_simple_evm_method(&mut vm, &evm_abi, false);
    call_simple_evm_method(&mut vm, &evm_abi, true);
    // Test that EraVM contracts can be called fine.
    call_eravm_counter(&mut vm);
}

fn call_simple_evm_method<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    evm_abi: &ethabi::Contract,
    fail: bool,
) {
    let test_fn = evm_abi.function("testCall").unwrap();
    let success_call = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Bool(fail)]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = vm.rich_accounts[0].get_l2_tx_for_execute(success_call, None);

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    if fail {
        assert_matches!(
            &vm_result.result,
            ExecutionResult::Revert { output: VmRevertReason::General { msg, .. } }
                if msg == "requested revert"
        );
    } else {
        assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
    }
    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert!(new_known_factory_deps.is_empty());
}

fn call_eravm_counter<VM: TestedVm>(vm: &mut VmTester<VM>) {
    let eravm_counter_slot =
        StorageKey::new(AccountTreeId::new(ERAVM_COUNTER_ADDRESS), H256::zero());
    let initial_value = vm.vm.read_storage(eravm_counter_slot);

    let eravm_counter_abi = load_test_contract_abi();
    let test_fn = eravm_counter_abi.function("increment").unwrap();
    let eravm_call = Execute {
        contract_address: Some(ERAVM_COUNTER_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Uint(3.into())]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = vm.rich_accounts[0].get_l2_tx_for_execute(eravm_call, None);

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
    assert_eq!(vm.vm.read_storage(eravm_counter_slot), initial_value + 3);
}

pub(crate) fn test_real_emulator_code_hash<VM: TestedVm>() {
    let (vm, deployed_evm_bytecode) = prepare_tester_with_real_emulator();
    let mut vm = vm.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let test_fn = evm_abi.function("testCodeHash").unwrap();

    let evm_bytecode_keccak_hash = web3::keccak256(&deployed_evm_bytecode);
    let test_execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn
            .encode_input(&[Token::FixedBytes(evm_bytecode_keccak_hash.to_vec())])
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
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
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
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
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

pub(crate) fn test_real_emulator_recursion<VM: TestedVm>() {
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
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

pub(crate) fn test_real_emulator_deployment<VM: TestedVm>() {
    let counter_bytecode =
        read_deployed_bytecode_from_path(EVM_COUNTER_CONTRACT_PATH.as_ref()).unwrap();

    let (proxy_counter_bytecode, proxy_counter_abi) = read_proxy_counter_contract();
    let mut vm = prepare_tester_with_real_emulator()
        .0
        .with_custom_contracts(vec![ContractToDeploy::new(
            proxy_counter_bytecode,
            ERAVM_COUNTER_ADDRESS,
        )])
        .build::<VM>();

    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let counter_address = deploy_and_call_evm_counter(&mut vm, &evm_abi, &counter_bytecode);
    // Manually set the `counter` address in `ProxyCounter` to the created contract.
    let counter_slot = StorageKey::new(AccountTreeId::new(ERAVM_COUNTER_ADDRESS), H256::zero());
    vm.storage
        .borrow_mut()
        .inner_mut()
        .set_value(counter_slot, address_to_h256(&counter_address));

    let account = &mut vm.rich_accounts[0];
    let test_fn = evm_abi.function("testCounterCall").unwrap();
    let test_execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Uint(3.into())]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = account.get_l2_tx_for_execute(test_execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert!(
        new_known_factory_deps.is_empty(),
        "{new_known_factory_deps:?}"
    );

    test_calling_evm_contract_from_era(&mut vm, &proxy_counter_abi, counter_address);
}

fn deploy_and_call_evm_counter<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    evm_abi: &ethabi::Contract,
    counter_bytecode: &[u8],
) -> Address {
    let test_fn = evm_abi.function("testDeploymentAndCall").unwrap();
    let counter_keccak_hash = web3::keccak256(counter_bytecode);
    let test_execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn
            .encode_input(&[Token::FixedBytes(counter_keccak_hash.to_vec())])
            .unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = vm.rich_accounts[0].get_l2_tx_for_execute(test_execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    let padded_bytecode = pad_evm_bytecode(counter_bytecode);
    assert_eq!(
        new_known_factory_deps,
        HashMap::from([(
            hash_evm_bytecode(&padded_bytecode),
            padded_bytecode.to_vec()
        )])
    );

    let counter_slot = StorageKey::new(AccountTreeId::new(EVM_ADDRESS), H256::zero());
    let counter_address = vm_result.logs.storage_logs.iter().find_map(|log| {
        let log = &log.log;
        (log.is_write() && log.key == counter_slot).then_some(log.value)
    });
    let counter_address =
        h256_to_account_address(&counter_address.expect("counter address not persisted"));
    assert_ne!(counter_address, Address::zero());

    counter_address
}

fn test_calling_evm_contract_from_era<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    proxy_counter_abi: &ethabi::Contract,
    counter_address: Address,
) {
    let account = &mut vm.rich_accounts[0];
    let counter_value_slot = StorageKey::new(AccountTreeId::new(counter_address), H256::zero());
    let initial_counter_value = vm.vm.read_storage(counter_value_slot);
    assert_ne!(initial_counter_value, 0.into());

    // Call the proxy counter, which should call the EVM counter.
    let proxy_increment_fn = proxy_counter_abi.function("testCounterCall").unwrap();
    let test_execute = Execute {
        contract_address: Some(ERAVM_COUNTER_ADDRESS),
        calldata: proxy_increment_fn
            .encode_input(&[Token::Uint(initial_counter_value)])
            .unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = account.get_l2_tx_for_execute(test_execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let counter_value = vm_result.logs.storage_logs.iter().find_map(|log| {
        let log = &log.log;
        (log.is_write() && log.key == counter_value_slot).then_some(log.value)
    });
    let counter_value = h256_to_u256(counter_value.expect("no counter value log"));
    assert!(counter_value > initial_counter_value);
}

pub(crate) fn test_era_vm_deployment_after_evm_execution<VM: TestedVm>() {
    let (proxy_counter_bytecode, _) = read_proxy_counter_contract();
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();

    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    call_simple_evm_method(&mut vm, &evm_abi, false);

    deploy_eravm_counter(&mut vm, &proxy_counter_bytecode, Address::zero());
}

pub(crate) fn test_era_vm_deployment_after_evm_deployment<VM: TestedVm>() {
    let counter_bytecode =
        read_deployed_bytecode_from_path(EVM_COUNTER_CONTRACT_PATH.as_ref()).unwrap();
    let (proxy_counter_bytecode, _) = read_proxy_counter_contract();
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();

    // Sanity check: deployment should succeed at the start of a batch.
    deploy_eravm_counter(&mut vm, &proxy_counter_bytecode, Address::zero());

    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let counter_address = deploy_and_call_evm_counter(&mut vm, &evm_abi, &counter_bytecode);

    deploy_eravm_counter(&mut vm, &proxy_counter_bytecode, counter_address);
}

pub(crate) fn test_deployment_with_partial_reverts<VM: TestedVm>() {
    for seed in [1, 10, 100, 1_000] {
        println!("Testing with RNG seed {seed}");
        let mut rng = StdRng::seed_from_u64(seed);
        test_deployment_with_partial_reverts_and_rng::<VM>(&mut rng);
    }
}

fn test_deployment_with_partial_reverts_and_rng<VM: TestedVm>(rng: &mut impl Rng) {
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let test_fn = evm_abi.function("testDeploymentWithPartialRevert").unwrap();
    let should_revert: Vec<_> = (0..10).map(|_| rng.gen::<bool>()).collect();
    let should_revert_tokens = should_revert.iter().copied().map(Token::Bool).collect();
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata: test_fn
                .encode_input(&[Token::Array(should_revert_tokens)])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{vm_result:?}");

    // All deployed contracts have the same bytecode.
    let new_factory_deps = vm_result.new_known_factory_deps.unwrap();
    let counter_bytecode =
        read_deployed_bytecode_from_path(EVM_COUNTER_CONTRACT_PATH.as_ref()).unwrap();
    let padded_evm_bytecode = pad_evm_bytecode(&counter_bytecode);
    assert_eq!(
        new_factory_deps,
        HashMap::from([(hash_evm_bytecode(&padded_evm_bytecode), padded_evm_bytecode)])
    );
}

fn deploy_eravm_counter<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    bytecode: &[u8],
    counter_address: Address,
) {
    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(
        bytecode,
        Some(&[Token::Address(counter_address)]),
        TxType::L2,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
    assert_ne!(
        vm.vm.read_storage(get_code_key(&deploy_tx.address)),
        0.into()
    );
}

pub(crate) fn test_calling_era_contract_from_evm<VM: TestedVm>() {
    let (vm, _) = prepare_tester_with_real_emulator();
    let era_counter = ContractToDeploy::new(read_test_contract(), ERAVM_COUNTER_ADDRESS);
    let counter_slot = StorageKey::new(AccountTreeId::new(EVM_ADDRESS), H256::zero());
    let mut vm = vm
        .with_custom_contracts(vec![era_counter])
        .with_storage_slots([(counter_slot, address_to_h256(&ERAVM_COUNTER_ADDRESS))])
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let evm_abi = load_contract(EVM_TEST_CONTRACT_PATH);
    let test_fn = evm_abi.function("testCounterCall").unwrap();
    let test_execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Uint(0.into())]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };
    let tx = account.get_l2_tx_for_execute(test_execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}
