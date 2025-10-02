use std::collections::HashMap;

use assert_matches::assert_matches;
use ethabi::Token;
use rand::{rngs::StdRng, Rng, SeedableRng};
use zksync_contracts::SystemContractCode;
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
};
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{
    bytecode::BytecodeHash,
    get_code_key, get_evm_code_hash_key, get_known_code_key, h256_to_u256,
    utils::{key_for_eth_balance, storage_key_for_eth_balance},
    web3, AccountTreeId, Address, Execute, StorageKey, H256, U256,
};

use super::{default_system_env, TestedVm, VmTester, VmTesterBuilder};
use crate::interface::{
    storage::InMemoryStorage, ExecutionResult, TxExecutionMode, VmExecutionResultAndLogs,
    VmInterfaceExt,
};

fn override_system_contracts(storage: &mut InMemoryStorage) {
    let mock_deployer = TestContract::mock_deployer().bytecode.to_vec();
    let mock_deployer_hash = BytecodeHash::for_bytecode(&mock_deployer).value();
    let mock_known_code_storage = TestContract::mock_known_code_storage().bytecode.to_vec();
    let mock_known_code_storage_hash = BytecodeHash::for_bytecode(&mock_known_code_storage).value();

    storage.set_value(get_code_key(&CONTRACT_DEPLOYER_ADDRESS), mock_deployer_hash);
    storage.set_value(
        get_known_code_key(&mock_deployer_hash),
        H256::from_low_u64_be(1),
    );
    storage.set_value(
        get_code_key(&KNOWN_CODES_STORAGE_ADDRESS),
        mock_known_code_storage_hash,
    );
    storage.set_value(
        get_known_code_key(&mock_known_code_storage_hash),
        H256::from_low_u64_be(1),
    );
    storage.store_factory_dep(mock_deployer_hash, mock_deployer);
    storage.store_factory_dep(mock_known_code_storage_hash, mock_known_code_storage);
}

#[derive(Debug)]
struct EvmTestBuilder {
    deploy_emulator: bool,
    storage: InMemoryStorage,
    evm_contract_addresses: Vec<Address>,
}

impl EvmTestBuilder {
    fn new(deploy_emulator: bool, evm_contract_address: Address) -> Self {
        Self {
            deploy_emulator,
            storage: InMemoryStorage::with_system_contracts(),
            evm_contract_addresses: vec![evm_contract_address],
        }
    }

    fn with_mock_deployer(mut self) -> Self {
        override_system_contracts(&mut self.storage);
        self
    }

    fn with_evm_address(mut self, address: Address) -> Self {
        self.evm_contract_addresses.push(address);
        self
    }

    fn build<VM: TestedVm>(self) -> VmTester<VM> {
        let mock_emulator = TestContract::mock_evm_emulator().bytecode.to_vec();
        let mut storage = self.storage;
        let mut system_env = default_system_env();
        if self.deploy_emulator {
            let evm_bytecode: Vec<_> = (0..32).collect();
            let evm_bytecode_hash =
                BytecodeHash::for_evm_bytecode(evm_bytecode.len(), &evm_bytecode).value();
            let keccak_bytecode_hash = H256(web3::keccak256(&evm_bytecode));
            storage.set_value(
                get_known_code_key(&evm_bytecode_hash),
                H256::from_low_u64_be(1),
            );
            for evm_address in self.evm_contract_addresses {
                storage.set_value(get_code_key(&evm_address), evm_bytecode_hash);
                storage.set_value(
                    get_evm_code_hash_key(evm_bytecode_hash),
                    keccak_bytecode_hash,
                );
            }

            system_env.base_system_smart_contracts.evm_emulator = Some(SystemContractCode {
                hash: BytecodeHash::for_bytecode(&mock_emulator).value(),
                code: mock_emulator,
            });
        } else {
            let emulator_hash = BytecodeHash::for_bytecode(&mock_emulator).value();
            storage.set_value(get_known_code_key(&emulator_hash), H256::from_low_u64_be(1));
            storage.store_factory_dep(emulator_hash, mock_emulator);

            for evm_address in self.evm_contract_addresses {
                storage.set_value(get_code_key(&evm_address), emulator_hash);
                // Set `isUserSpace` in the emulator storage to `true`, so that it skips emulator-specific checks
                storage.set_value(
                    StorageKey::new(AccountTreeId::new(evm_address), H256::zero()),
                    H256::from_low_u64_be(1),
                );
            }
        }

        VmTesterBuilder::new()
            .with_system_env(system_env)
            .with_storage(storage)
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
            .build::<VM>()
    }
}

pub(crate) fn test_tracing_evm_contract_deployment<VM: TestedVm>() {
    let mut storage = InMemoryStorage::with_system_contracts();
    override_system_contracts(&mut storage);

    let mut system_env = default_system_env();
    // The EVM emulator will not be accessed, so we set it to a dummy value.
    system_env.base_system_smart_contracts.evm_emulator =
        Some(system_env.base_system_smart_contracts.default_aa.clone());
    let mut vm = VmTesterBuilder::new()
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let args = [Token::Bytes((0..32).collect())];
    let evm_bytecode = ethabi::encode(&args);
    let expected_bytecode_hash =
        BytecodeHash::for_evm_bytecode(evm_bytecode.len(), &evm_bytecode).value();
    let execute = Execute::for_deploy(expected_bytecode_hash, vec![0; 32], &args);
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    // The EraVM contract also deployed in a transaction should be filtered out
    assert_eq!(
        vm_result.dynamic_factory_deps,
        HashMap::from([(expected_bytecode_hash, evm_bytecode)])
    );

    // "Deploy" a bytecode in another transaction and check that the first tx doesn't interfere with the returned `dynamic_factory_deps`.
    let args = [Token::Bytes((0..32).rev().collect())];
    let evm_bytecode = ethabi::encode(&args);
    let expected_bytecode_hash =
        BytecodeHash::for_evm_bytecode(evm_bytecode.len(), &evm_bytecode).value();
    let execute = Execute::for_deploy(expected_bytecode_hash, vec![0; 32], &args);
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    assert_eq!(
        vm_result.dynamic_factory_deps,
        HashMap::from([(expected_bytecode_hash, evm_bytecode)])
    );
}

pub(crate) fn test_mock_emulator_basics<VM: TestedVm>() {
    let called_address = Address::repeat_byte(0x23);
    let mut vm = EvmTestBuilder::new(true, called_address).build::<VM>();
    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(called_address),
            calldata: vec![],
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

const RECIPIENT_ADDRESS: Address = Address::repeat_byte(0x12);

/// `deploy_emulator = false` here and below tests the mock emulator as an ordinary contract (i.e., sanity-checks its logic).
pub(crate) fn test_mock_emulator_with_payment<VM: TestedVm>(deploy_emulator: bool) {
    let mut vm = EvmTestBuilder::new(deploy_emulator, RECIPIENT_ADDRESS).build::<VM>();

    let mut current_balance = U256::zero();
    for i in 1_u64..=5 {
        let transferred_value = (1_000_000_000 * i).into();
        let vm_result = test_payment(
            &mut vm,
            &TestContract::mock_evm_emulator().abi,
            &mut current_balance,
            transferred_value,
        );

        let balance_storage_logs = vm_result.logs.storage_logs.iter().filter_map(|log| {
            (*log.log.key.address() == L2_BASE_TOKEN_ADDRESS)
                .then_some((*log.log.key.key(), h256_to_u256(log.log.value)))
        });
        let balances: HashMap<_, _> = balance_storage_logs.collect();
        assert_eq!(
            balances[&key_for_eth_balance(&RECIPIENT_ADDRESS)],
            current_balance
        );
    }
}

fn test_payment<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    mock_emulator_abi: &ethabi::Contract,
    balance: &mut U256,
    transferred_value: U256,
) -> VmExecutionResultAndLogs {
    *balance += transferred_value;
    let test_payment_fn = mock_emulator_abi.function("testPayment").unwrap();
    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(RECIPIENT_ADDRESS),
            calldata: test_payment_fn
                .encode_input(&[Token::Uint(transferred_value), Token::Uint(*balance)])
                .unwrap(),
            value: transferred_value,
            factory_deps: vec![],
        },
        None,
    );

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{vm_result:?}");
    vm_result
}

pub(crate) fn test_mock_emulator_with_recursion<VM: TestedVm>(
    deploy_emulator: bool,
    is_external: bool,
) {
    let mock_emulator_abi = &TestContract::mock_evm_emulator().abi;
    let recipient_address = Address::repeat_byte(0x12);
    let mut vm = EvmTestBuilder::new(deploy_emulator, recipient_address).build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let test_recursion_fn = mock_emulator_abi
        .function(if is_external {
            "testExternalRecursion"
        } else {
            "testRecursion"
        })
        .unwrap();
    let mut expected_value = U256::one();
    let depth = 50_u32;
    for i in 2..=depth {
        expected_value *= i;
    }

    let factory_deps = if is_external {
        vec![TestContract::recursive_test().bytecode.to_vec()]
    } else {
        vec![]
    };
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(recipient_address),
            calldata: test_recursion_fn
                .encode_input(&[Token::Uint(depth.into()), Token::Uint(expected_value)])
                .unwrap(),
            value: 0.into(),
            factory_deps,
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(tx, true);
    assert!(!vm_result.result.is_failed(), "{vm_result:?}");
}

pub(crate) fn test_calling_to_mock_emulator_from_native_contract<VM: TestedVm>() {
    let recipient_address = Address::repeat_byte(0x12);
    let mut vm = EvmTestBuilder::new(true, recipient_address).build::<VM>();
    let account = &mut vm.rich_accounts[0];

    // Deploy a native contract.
    let deploy_tx = account.get_deploy_tx(
        TestContract::recursive_test().bytecode,
        Some(&[Token::Address(recipient_address)]),
        TxType::L2,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    // Call from the native contract to the EVM emulator.
    let test_fn = TestContract::recursive_test().function("recurse");
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(deploy_tx.address),
            calldata: test_fn.encode_input(&[Token::Uint(50.into())]).unwrap(),
            value: Default::default(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

pub(crate) fn test_mock_emulator_with_deployment<VM: TestedVm>(revert: bool) {
    let contract_address = Address::repeat_byte(0xaa);
    let mut vm = EvmTestBuilder::new(true, contract_address)
        .with_mock_deployer()
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let mock_emulator_abi = &TestContract::mock_evm_emulator().abi;
    let new_evm_bytecode = vec![0xfe; 96];
    let new_evm_bytecode_hash =
        BytecodeHash::for_evm_bytecode(new_evm_bytecode.len(), &new_evm_bytecode).value();

    let test_fn = mock_emulator_abi.function("testDeploymentAndCall").unwrap();
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: test_fn
                .encode_input(&[
                    Token::FixedBytes(new_evm_bytecode_hash.0.into()),
                    Token::Bytes(new_evm_bytecode.clone()),
                    Token::Bool(revert),
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

    assert_eq!(vm_result.result.is_failed(), revert, "{vm_result:?}");
    let expected_dynamic_deps = if revert {
        HashMap::new()
    } else {
        HashMap::from([(new_evm_bytecode_hash, new_evm_bytecode)])
    };
    assert_eq!(vm_result.dynamic_factory_deps, expected_dynamic_deps);

    // Test that a following transaction can decommit / call EVM contracts deployed in the previous transaction.
    let test_fn = mock_emulator_abi
        .function("testCallToPreviousDeployment")
        .unwrap();
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: test_fn.encode_input(&[]).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);

    if revert {
        assert_matches!(
            &vm_result.result,
            ExecutionResult::Revert { output }
                if output.to_string().contains("contract code length")
        );
    } else {
        assert!(!vm_result.result.is_failed(), "{vm_result:?}");
    }
    assert!(vm_result.dynamic_factory_deps.is_empty(), "{vm_result:?}");
}

fn encode_deployment(hash: H256, bytecode: Vec<u8>) -> Token {
    assert_eq!(bytecode.len(), 32);
    Token::Tuple(vec![
        Token::FixedBytes(hash.0.to_vec()),
        Token::FixedBytes(bytecode),
    ])
}

pub(crate) fn test_mock_emulator_with_recursive_deployment<VM: TestedVm>() {
    let contract_address = Address::repeat_byte(0xaa);
    let mut vm = EvmTestBuilder::new(true, contract_address)
        .with_mock_deployer()
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let mock_emulator_abi = &TestContract::mock_evm_emulator().abi;
    let bytecodes: HashMap<_, _> = (0_u8..10)
        .map(|byte| {
            let bytecode = vec![byte; 32];
            (
                BytecodeHash::for_evm_bytecode(bytecode.len(), &bytecode).value(),
                bytecode,
            )
        })
        .collect();
    let test_fn = mock_emulator_abi
        .function("testRecursiveDeployment")
        .unwrap();
    let deployments: Vec<_> = bytecodes
        .iter()
        .map(|(hash, code)| encode_deployment(*hash, code.clone()))
        .collect();
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: test_fn.encode_input(&[Token::Array(deployments)]).unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );

    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(test_tx, true);
    assert!(!vm_result.result.is_failed(), "{vm_result:?}");
    assert_eq!(vm_result.dynamic_factory_deps, bytecodes);
}

pub(crate) fn test_mock_emulator_with_partial_reverts<VM: TestedVm>() {
    for seed in [1, 10, 100, 1_000] {
        println!("Testing with RNG seed {seed}");
        let mut rng = StdRng::seed_from_u64(seed);
        test_mock_emulator_with_partial_reverts_and_rng::<VM>(&mut rng);
    }
}

fn test_mock_emulator_with_partial_reverts_and_rng<VM: TestedVm>(rng: &mut impl Rng) {
    let contract_address = Address::repeat_byte(0xaa);
    let mut vm = EvmTestBuilder::new(true, contract_address)
        .with_mock_deployer()
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let mock_emulator_abi = &TestContract::mock_evm_emulator().abi;
    let all_bytecodes: HashMap<_, _> = (0_u8..10)
        .map(|_| {
            let bytecode = vec![rng.gen(); 32];
            (
                BytecodeHash::for_evm_bytecode(bytecode.len(), &bytecode).value(),
                bytecode,
            )
        })
        .collect();
    let should_revert: Vec<_> = (0..10).map(|_| rng.gen::<bool>()).collect();

    let test_fn = mock_emulator_abi
        .function("testDeploymentWithPartialRevert")
        .unwrap();
    let deployments: Vec<_> = all_bytecodes
        .iter()
        .map(|(hash, code)| encode_deployment(*hash, code.clone()))
        .collect();
    let revert_tokens: Vec<_> = should_revert.iter().copied().map(Token::Bool).collect();

    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: test_fn
                .encode_input(&[Token::Array(deployments), Token::Array(revert_tokens)])
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

    let dynamic_deps = &vm_result.dynamic_factory_deps;
    assert_eq!(
        dynamic_deps.len(),
        should_revert
            .iter()
            .map(|flag| !flag as usize)
            .sum::<usize>(),
        "{dynamic_deps:?}"
    );
    for ((bytecode_hash, bytecode), &should_revert) in all_bytecodes.iter().zip(&should_revert) {
        assert_eq!(
            dynamic_deps.get(bytecode_hash),
            (!should_revert).then_some(bytecode),
            "hash={bytecode_hash:?}, deps={dynamic_deps:?}"
        );
    }
}

pub(crate) fn test_mock_emulator_with_delegate_call<VM: TestedVm>() {
    let evm_contract_address = Address::repeat_byte(0xaa);
    let other_evm_contract_address = Address::repeat_byte(0xbb);
    let mut builder = EvmTestBuilder::new(true, evm_contract_address);
    builder.storage.set_value(
        storage_key_for_eth_balance(&evm_contract_address),
        H256::from_low_u64_be(1_000_000),
    );
    builder.storage.set_value(
        storage_key_for_eth_balance(&other_evm_contract_address),
        H256::from_low_u64_be(2_000_000),
    );
    let mut vm = builder
        .with_evm_address(other_evm_contract_address)
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    // Deploy a native contract.
    let deploy_tx =
        account.get_deploy_tx(TestContract::increment_test().bytecode, None, TxType::L2);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let test_fn = TestContract::increment_test().function("testDelegateCall");
    // Delegate to the native contract from EVM.
    test_delegate_call(&mut vm, test_fn, evm_contract_address, deploy_tx.address);
    // Delegate to EVM from the native contract.
    test_delegate_call(&mut vm, test_fn, deploy_tx.address, evm_contract_address);
    // Delegate to EVM from EVM.
    test_delegate_call(
        &mut vm,
        test_fn,
        evm_contract_address,
        other_evm_contract_address,
    );
}

fn test_delegate_call<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    test_fn: &ethabi::Function,
    from: Address,
    to: Address,
) {
    let account = &mut vm.rich_accounts[0];
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(from),
            calldata: test_fn.encode_input(&[Token::Address(to)]).unwrap(),
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

pub(crate) fn test_mock_emulator_with_static_call<VM: TestedVm>() {
    let evm_contract_address = Address::repeat_byte(0xaa);
    let other_evm_contract_address = Address::repeat_byte(0xbb);
    let mut builder = EvmTestBuilder::new(true, evm_contract_address);
    builder.storage.set_value(
        storage_key_for_eth_balance(&evm_contract_address),
        H256::from_low_u64_be(1_000_000),
    );
    builder.storage.set_value(
        storage_key_for_eth_balance(&other_evm_contract_address),
        H256::from_low_u64_be(2_000_000),
    );
    // Set differing read values for tested contracts. The slot index is defined in the contract.
    let value_slot = H256::from_low_u64_be(0x123);
    builder.storage.set_value(
        StorageKey::new(AccountTreeId::new(evm_contract_address), value_slot),
        H256::from_low_u64_be(100),
    );
    builder.storage.set_value(
        StorageKey::new(AccountTreeId::new(other_evm_contract_address), value_slot),
        H256::from_low_u64_be(200),
    );
    let mut vm = builder
        .with_evm_address(other_evm_contract_address)
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    // Deploy a native contract.
    let deploy_tx =
        account.get_deploy_tx(TestContract::increment_test().bytecode, None, TxType::L2);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let test_fn = TestContract::increment_test().function("testStaticCall");
    // Call to the native contract from EVM.
    test_static_call(&mut vm, test_fn, evm_contract_address, deploy_tx.address, 0);
    // Call to EVM from the native contract.
    test_static_call(
        &mut vm,
        test_fn,
        deploy_tx.address,
        evm_contract_address,
        100,
    );
    // Call to EVM from EVM.
    test_static_call(
        &mut vm,
        test_fn,
        evm_contract_address,
        other_evm_contract_address,
        200,
    );
}

fn test_static_call<VM: TestedVm>(
    vm: &mut VmTester<VM>,
    test_fn: &ethabi::Function,
    from: Address,
    to: Address,
    expected_value: u64,
) {
    let account = &mut vm.rich_accounts[0];
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(from),
            calldata: test_fn
                .encode_input(&[Token::Address(to), Token::Uint(expected_value.into())])
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
