use std::collections::HashMap;

use assert_matches::assert_matches;
use circuit_sequencer_api::geometry_config::ProtocolGeometry;
use ethabi::{ParamType, Token};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_test_contracts::{Account, TestContract, TestEvmContract, TxType};
use zksync_types::{
    address_to_h256,
    block::L2BlockHasher,
    bytecode::{pad_evm_bytecode, BytecodeHash},
    get_code_key, get_known_code_key, get_nonce_key, h256_to_address, h256_to_u256, u256_to_h256,
    utils::{decompose_full_nonce, deployed_address_evm_create},
    web3, AccountTreeId, Address, Execute, K256PrivateKey, L2BlockNumber, ProtocolVersionId,
    StorageKey, H256, U256,
};
use zksync_vm_interface::{InspectExecutionMode, VmEvent};

use super::{ContractToDeploy, TestedVm, VmTester, VmTesterBuilder};
use crate::{
    interface::{ExecutionResult, L2BlockEnv, TxExecutionMode, VmInterfaceExt, VmRevertReason},
    utils::get_batch_base_fee,
};

const EVM_ADDRESS: Address = Address::repeat_byte(1);
const ERAVM_ADDRESS: Address = Address::repeat_byte(2);

pub(crate) fn test_evm_deployment_tx<VM: TestedVm>() {
    let mut vm: VmTester<VM> = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_evm_emulator()
        .build();

    let account = &mut vm.rich_accounts[0];
    let initial_counter = 3.into();
    let tx = account.get_evm_deploy_tx(
        TestEvmContract::counter().init_bytecode.to_vec(),
        &TestEvmContract::counter().abi,
        &[Token::Uint(initial_counter)],
    );
    vm.vm.push_transaction(tx.into());

    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "{result:#?}");

    let deployed_bytecode = TestEvmContract::counter().deployed_bytecode;
    let padded_bytecode = pad_evm_bytecode(deployed_bytecode);
    let expected_bytecode_hash =
        BytecodeHash::for_evm_bytecode(deployed_bytecode.len(), &padded_bytecode).value();
    let expected_address = deployed_address_evm_create(account.address, 0.into());

    let stored_bytecode_hash = u256_to_h256(vm.vm.read_storage(get_code_key(&expected_address)));
    assert_eq!(stored_bytecode_hash, expected_bytecode_hash);
    assert_eq!(
        vm.vm
            .read_storage(get_known_code_key(&expected_bytecode_hash)),
        1.into()
    );
    let stored_nonce = vm.vm.read_storage(get_nonce_key(&account.address));
    assert_eq!(stored_nonce, 1.into()); // deployment nonce is not used for EVM deployments

    // Test contract storage
    let stored_counter = vm.vm.read_storage(StorageKey::new(
        AccountTreeId::new(expected_address),
        H256::zero(),
    ));
    assert_eq!(stored_counter, initial_counter);

    assert_eq!(
        result.dynamic_factory_deps,
        HashMap::from([(expected_bytecode_hash, padded_bytecode)])
    );
}

fn prepare_tester_with_real_emulator() -> (VmTesterBuilder, &'static [u8]) {
    let deployed_evm_bytecode = TestEvmContract::evm_tester().deployed_bytecode;

    let tester_builder = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_evm_contracts(vec![ContractToDeploy::new(
            deployed_evm_bytecode.to_vec(),
            EVM_ADDRESS,
        )]);
    (tester_builder, deployed_evm_bytecode)
}

pub(crate) fn test_evm_bytecode_decommit<VM: TestedVm>() {
    let (vm_builder, deployed_evm_bytecode) = prepare_tester_with_real_emulator();
    let mut vm: VmTester<VM> = vm_builder
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::precompiles_test().bytecode.to_vec(),
            ERAVM_ADDRESS,
        )])
        .build();

    let account = &mut vm.rich_accounts[0];
    let call_code_oracle_function = TestContract::precompiles_test().function("callCodeOracle");
    let padded_bytecode = pad_evm_bytecode(deployed_evm_bytecode);
    let evm_bytecode_hash =
        BytecodeHash::for_evm_bytecode(deployed_evm_bytecode.len(), &padded_bytecode).value();
    let evm_bytecode_keccak_hash = H256(web3::keccak256(&padded_bytecode));

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(ERAVM_ADDRESS),
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(evm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(evm_bytecode_keccak_hash.0.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );
}

pub(crate) fn test_real_emulator_basics<VM: TestedVm>() {
    let eravm_counter_bytecode = TestContract::counter().bytecode.to_vec();
    let mut vm = prepare_tester_with_real_emulator()
        .0
        .with_custom_contracts(vec![ContractToDeploy::new(
            eravm_counter_bytecode,
            ERAVM_ADDRESS,
        )])
        .build::<VM>();
    let evm_abi = &TestEvmContract::evm_tester().abi;

    call_simple_evm_method(&mut vm, evm_abi, false);
    call_simple_evm_method(&mut vm, evm_abi, true);
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
    assert!(vm_result.dynamic_factory_deps.is_empty());
}

fn call_eravm_counter<VM: TestedVm>(vm: &mut VmTester<VM>) {
    let eravm_counter_slot = StorageKey::new(AccountTreeId::new(ERAVM_ADDRESS), H256::zero());
    let initial_value = vm.vm.read_storage(eravm_counter_slot);

    let test_fn = TestContract::counter().function("increment");
    let eravm_call = Execute {
        contract_address: Some(ERAVM_ADDRESS),
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
    let test_fn = TestEvmContract::evm_tester().function("testCodeHash");

    let evm_bytecode_keccak_hash = web3::keccak256(deployed_evm_bytecode);
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
    let evm_abi = &TestEvmContract::evm_tester().abi;

    let first_block = vm.l1_batch_env.first_l2_block;
    let tx = account.get_l2_tx_for_execute(create_test_block_execute(evm_abi, &first_block), None);
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

    let tx = account.get_l2_tx_for_execute(create_test_block_execute(evm_abi, &second_block), None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

pub(crate) fn test_real_emulator_msg_info<VM: TestedVm>() {
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let test_fn = TestEvmContract::evm_tester().function("testMsgInfo");
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

pub(crate) fn test_real_emulator_gas_management<VM: TestedVm>() {
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let test_fn = TestEvmContract::evm_tester().function("testGasManagement");
    let fee = Account::default_fee();
    let execute = Execute {
        contract_address: Some(EVM_ADDRESS),
        calldata: test_fn.encode_input(&[Token::Uint(fee.gas_limit)]).unwrap(),
        value: 0.into(),
        factory_deps: vec![],
    };

    let tx = account.get_l2_tx_for_execute(execute, Some(fee));
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

pub(crate) fn test_real_emulator_recursion<VM: TestedVm>() {
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let test_fn = TestEvmContract::evm_tester().function("testRecursion");

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
        assert!(vm_result.dynamic_factory_deps.is_empty());
    }
}

pub(crate) fn test_real_emulator_deployment<VM: TestedVm>() {
    let counter_bytecode = TestEvmContract::counter().deployed_bytecode;

    let mut vm = prepare_tester_with_real_emulator()
        .0
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::proxy_counter().bytecode.to_vec(),
            ERAVM_ADDRESS,
        )])
        .build::<VM>();

    let evm_abi = &TestEvmContract::evm_tester().abi;
    let counter_address = deploy_and_call_evm_counter(&mut vm, evm_abi, counter_bytecode);
    // Manually set the `counter` address in `ProxyCounter` to the created contract.
    let counter_slot = StorageKey::new(AccountTreeId::new(ERAVM_ADDRESS), H256::zero());
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

    let new_known_factory_deps = vm_result.dynamic_factory_deps;
    assert!(
        new_known_factory_deps.is_empty(),
        "{new_known_factory_deps:?}"
    );

    let proxy_counter_abi = &TestContract::proxy_counter().abi;
    test_calling_evm_contract_from_era(&mut vm, proxy_counter_abi, counter_address);
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
    assert!(!vm_result.result.is_failed(), "{:#?}", vm_result);

    let new_known_factory_deps = vm_result.dynamic_factory_deps;
    let padded_bytecode = pad_evm_bytecode(counter_bytecode);
    assert_eq!(
        new_known_factory_deps,
        HashMap::from([(
            BytecodeHash::for_evm_bytecode(counter_bytecode.len(), &padded_bytecode).value(),
            padded_bytecode.to_vec()
        )])
    );

    let counter_slot = StorageKey::new(AccountTreeId::new(EVM_ADDRESS), H256::zero());
    let counter_address = vm_result.logs.storage_logs.iter().find_map(|log| {
        let log = &log.log;
        (log.is_write() && log.key == counter_slot).then_some(log.value)
    });
    let counter_address = h256_to_address(&counter_address.expect("counter address not persisted"));
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
        contract_address: Some(ERAVM_ADDRESS),
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
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();

    let evm_abi = &TestEvmContract::evm_tester().abi;
    call_simple_evm_method(&mut vm, evm_abi, false);

    deploy_eravm_counter(
        &mut vm,
        TestContract::proxy_counter().bytecode,
        Address::zero(),
    );
}

pub(crate) fn test_era_vm_deployment_after_evm_deployment<VM: TestedVm>() {
    let counter_bytecode = TestEvmContract::counter().deployed_bytecode;
    let proxy_counter_bytecode = TestContract::proxy_counter().bytecode;
    let mut vm = prepare_tester_with_real_emulator().0.build::<VM>();

    // Sanity check: deployment should succeed at the start of a batch.
    deploy_eravm_counter(&mut vm, proxy_counter_bytecode, Address::zero());

    let evm_abi = &TestEvmContract::evm_tester().abi;
    let counter_address = deploy_and_call_evm_counter(&mut vm, evm_abi, counter_bytecode);

    deploy_eravm_counter(&mut vm, proxy_counter_bytecode, counter_address);
}

fn evm_create2_address(
    sender: Address,
    salt: H256,
    creation_bytecode: &[u8],
    constructor_args: &[Token],
) -> Address {
    let mut creation_bytecode_and_args = creation_bytecode.to_vec();
    creation_bytecode_and_args.extend_from_slice(&ethabi::encode(constructor_args));

    let mut buffer = vec![0xff_u8];
    buffer.extend_from_slice(sender.as_bytes());
    buffer.extend_from_slice(salt.as_bytes());
    buffer.extend_from_slice(&web3::keccak256(&creation_bytecode_and_args));
    let hash_digest = web3::keccak256(&buffer);
    Address::from_slice(&hash_digest[12..])
}

pub(crate) fn test_create2_deployment_in_evm<VM: TestedVm>() {
    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let test_fn = TestEvmContract::evm_tester().function("testCreate2Deployment");

    let mut rng = StdRng::seed_from_u64(123);
    for _ in 0..10 {
        let salt = H256(rng.gen());
        let constructor_arg = U256(rng.gen());
        let expected_address = evm_create2_address(
            EVM_ADDRESS,
            salt,
            TestEvmContract::counter().init_bytecode,
            &[Token::Uint(constructor_arg)],
        );

        let test_tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(EVM_ADDRESS),
                calldata: test_fn
                    .encode_input(&[
                        Token::FixedBytes(salt.as_bytes().to_vec()),
                        Token::Uint(constructor_arg),
                        Token::Address(expected_address),
                    ])
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
    }
}

pub(crate) fn test_reusing_create_address_in_evm<VM: TestedVm>() {
    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let expected_address = deployed_address_evm_create(EVM_ADDRESS, 0.into());
    let test_fn = TestEvmContract::evm_tester().function("testReusingCreateAddress");
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata: test_fn
                .encode_input(&[Token::Address(expected_address)])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{vm_result:#?}");
}

pub(crate) fn test_reusing_create2_salt_in_evm<VM: TestedVm>() {
    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let test_fn = TestEvmContract::evm_tester().function("testReusingCreate2Salt");
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata: test_fn.encode_input(&[]).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{vm_result:#?}");
}

pub(crate) fn test_deployment_with_partial_reverts<VM: TestedVm>() {
    for seed in [1, 10, 100, 1_000] {
        println!("Testing with RNG seed {seed}");
        let mut rng = StdRng::seed_from_u64(seed);
        test_deployment_with_partial_reverts_and_rng::<VM>(&mut rng);
    }
}

fn test_deployment_with_partial_reverts_and_rng<VM: TestedVm>(rng: &mut impl Rng) {
    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let test_fn = TestEvmContract::evm_tester().function("testDeploymentWithPartialRevert");
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
    let new_factory_deps = vm_result.dynamic_factory_deps;
    let counter_bytecode = TestEvmContract::counter().deployed_bytecode;
    let padded_evm_bytecode = pad_evm_bytecode(counter_bytecode);
    let evm_bytecode_hash =
        BytecodeHash::for_evm_bytecode(counter_bytecode.len(), &padded_evm_bytecode).value();
    assert_eq!(
        new_factory_deps,
        HashMap::from([(evm_bytecode_hash, padded_evm_bytecode)])
    );

    // Check deployment events.
    let expected_addresses: Vec<_> = should_revert
        .iter()
        .enumerate()
        .filter_map(|(i, &should_revert)| {
            if should_revert {
                return None;
            }
            // **Important.* Should correspond to contract creation logic in Solidity
            let salt = H256::from_low_u64_be(i as u64);
            Some(evm_create2_address(
                EVM_ADDRESS,
                salt,
                TestEvmContract::counter().init_bytecode,
                &[Token::Uint(0.into())],
            ))
        })
        .collect();

    let deploy_events = vm_result.logs.events.iter().filter(|event| {
        event.indexed_topics.first() == Some(&VmEvent::DEPLOY_EVENT_SIGNATURE)
            && event.address == CONTRACT_DEPLOYER_ADDRESS
    });
    let deployed_addresses = deploy_events.map(|event| {
        assert_eq!(event.indexed_topics.len(), 4);
        let deployer_address = h256_to_address(&event.indexed_topics[1]);
        assert_eq!(deployer_address, EVM_ADDRESS);
        let bytecode_hash = event.indexed_topics[2];
        assert_eq!(bytecode_hash, evm_bytecode_hash);
        h256_to_address(&event.indexed_topics[3])
    });
    let deployed_addresses: Vec<_> = deployed_addresses.collect();
    assert_eq!(deployed_addresses, expected_addresses);
}

fn deploy_eravm_counter<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    bytecode: &[u8],
    counter_address: Address,
) {
    let account = &mut vm.rich_accounts[0];
    // Sanity check: account nonces must match ones in the storage
    let full_nonce = vm.vm.read_storage(get_nonce_key(&account.address));
    let (min_nonce, deploy_nonce) = decompose_full_nonce(full_nonce);
    assert_eq!(min_nonce.as_u32(), account.nonce.0);
    assert_eq!(deploy_nonce.as_u32(), account.deploy_nonce.0);

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
    let era_counter =
        ContractToDeploy::new(TestContract::counter().bytecode.to_vec(), ERAVM_ADDRESS);
    let counter_slot = StorageKey::new(AccountTreeId::new(EVM_ADDRESS), H256::zero());
    let mut vm = vm
        .with_custom_contracts(vec![era_counter])
        .with_storage_slots([(counter_slot, address_to_h256(&ERAVM_ADDRESS))])
        .build::<VM>();

    let account = &mut vm.rich_accounts[0];
    let test_fn = TestEvmContract::evm_tester().function("testCounterCall");
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

pub(crate) fn test_far_calls_from_evm_contract<VM: TestedVm>() {
    let era_tester = TestContract::eravm_tester().bytecode.to_vec();
    let mut vm = prepare_tester_with_real_emulator()
        .0
        .with_custom_contracts(vec![ContractToDeploy::new(era_tester, ERAVM_ADDRESS)])
        .build::<VM>();
    let evm_abi = &TestEvmContract::evm_tester().abi;

    println!("Testing EVM -> EVM far calls");
    call_far_call_test(&mut vm, evm_abi, EVM_ADDRESS, EVM_ADDRESS);
    println!("Testing EVM -> EraVM far calls");
    call_far_call_test(&mut vm, evm_abi, EVM_ADDRESS, ERAVM_ADDRESS);
    println!("Testing EraVM -> EraVM far calls (sanity check)");
    call_far_call_test(&mut vm, evm_abi, ERAVM_ADDRESS, ERAVM_ADDRESS);
    println!("Testing EraVM -> EVM far calls");
    call_far_call_test(&mut vm, evm_abi, ERAVM_ADDRESS, EVM_ADDRESS);
}

fn call_far_call_test<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    evm_abi: &ethabi::Contract,
    tester: Address,
    target: Address,
) {
    let test_fn = evm_abi.function("testFarCalls").unwrap();
    let is_evm_target = target == EVM_ADDRESS;
    let test_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(tester),
            calldata: test_fn
                .encode_input(&[Token::Address(target), Token::Bool(is_evm_target)])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    GasError::assert_success(&vm_result.result);
}

#[derive(Debug)]
#[allow(dead_code)] // fields are output in panic messages via `Debug`
enum GasError {
    TooMuchGas { expected: U256, actual: U256 },
    TooFewGas { expected: U256, actual: U256 },
}

impl GasError {
    fn parse(raw: &[u8]) -> Option<Self> {
        let too_much_gas_signature =
            ethabi::short_signature("TooMuchGas", &[ParamType::Uint(256), ParamType::Uint(256)]);
        let too_few_gas_signature =
            ethabi::short_signature("TooFewGas", &[ParamType::Uint(256), ParamType::Uint(256)]);

        let (signature, data) = raw.split_at(4);
        if signature == too_much_gas_signature {
            let (expected, actual) = Self::decode_two_ints(data);
            Some(Self::TooMuchGas { expected, actual })
        } else if signature == too_few_gas_signature {
            let (expected, actual) = Self::decode_two_ints(data);
            Some(Self::TooFewGas { expected, actual })
        } else {
            None
        }
    }

    fn decode_two_ints(data: &[u8]) -> (U256, U256) {
        let tokens = ethabi::decode(&[ParamType::Uint(256), ParamType::Uint(256)], data).unwrap();
        match tokens.as_slice() {
            [Token::Uint(x), Token::Uint(y)] => (*x, *y),
            _ => panic!("unexpected tokens"),
        }
    }

    fn assert_success(result: &ExecutionResult) {
        match result {
            ExecutionResult::Success { .. } => { /* OK */ }
            ExecutionResult::Revert {
                output: VmRevertReason::Unknown { data, .. },
            } => {
                if let Some(err) = Self::parse(data) {
                    panic!("{err:?}");
                }
                panic!("unexpected result: {result:?}");
            }
            ExecutionResult::Revert { .. } | ExecutionResult::Halt { .. } => {
                panic!("unexpected result: {result:?}");
            }
        }
    }
}

pub(crate) fn test_emitted_events<VM: TestedVm>() {
    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build();
    let tester = TestEvmContract::evm_tester();

    let test_fn = tester.function("testEvents");
    let test_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata: test_fn.encode_input(&[]).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let events: Vec<_> = vm_result
        .logs
        .events
        .iter()
        .filter(|event| event.address == EVM_ADDRESS)
        .collect();
    assert_eq!(events.len(), 2, "{events:#?}");
    assert_eq!(
        events[0].indexed_topics,
        [tester.abi.event("SimpleEvent").unwrap().signature()]
    );
    let complex_event = tester.abi.event("ComplexEvent").unwrap();
    assert_eq!(
        events[1].indexed_topics,
        [
            complex_event.signature(),
            H256(web3::keccak256("Test".as_bytes()))
        ]
    );
    let expected_value = ethabi::encode(&[Token::String("Test".into())]);
    assert_eq!(events[1].value, expected_value);
}

pub(crate) fn test_calling_sha256_precompile<VM: TestedVm>() {
    use zk_evm_1_5_0::sha2::{Digest, Sha256};

    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build();
    let tester = TestEvmContract::evm_tester();

    let test_fn = tester.function("testSha256");
    let input = b"Test SHA-256 input";
    let expected_output = Sha256::digest(input);
    let test_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata: test_fn
                .encode_input(&[
                    Token::Bytes(input.to_vec()),
                    Token::FixedBytes(expected_output.to_vec()),
                ])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let sha256_stats = vm_result.statistics.circuit_statistic.sha256;
    let sha_count =
        sha256_stats * ProtocolGeometry::V1_5_0.config().cycles_per_sha256_circuit as f32;
    assert_eq!(sha_count.round(), 1.0);
}

pub(crate) fn test_calling_ecrecover_precompile<VM: TestedVm>() {
    let mut vm: VmTester<VM> = prepare_tester_with_real_emulator().0.build();
    let tester = TestEvmContract::evm_tester();

    let sk = K256PrivateKey::from_bytes(H256::repeat_byte(1)).unwrap();
    let message_digest = H256(web3::keccak256(b"Test message"));
    let signature = sk.sign_web3_message(&message_digest);
    assert!(signature.v <= 1, "{signature:?}");

    let test_fn = tester.function("testEcrecover");
    let calldata = test_fn
        .encode_input(&[
            Token::FixedBytes(message_digest.0.to_vec()),
            Token::Uint((signature.v + 27).into()),
            Token::FixedBytes(signature.r.0.to_vec()),
            Token::FixedBytes(signature.s.0.to_vec()),
            Token::Address(sk.address()),
        ])
        .unwrap();
    let test_tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(EVM_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let ecrecover_stats = vm_result.statistics.circuit_statistic.ecrecover;
    let ecrecover_count = ecrecover_stats
        * ProtocolGeometry::V1_5_0
            .config()
            .cycles_per_ecrecover_circuit as f32;
    // There's another `ecrecover` call in the default AA tx validation logic
    assert_eq!(ecrecover_count.round(), 2.0);
}
