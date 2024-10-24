use std::collections::HashMap;

use ethabi::Token;
use zksync_contracts::{load_contract, read_bytecode, SystemContractCode};
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
};
use zksync_test_account::TxType;
use zksync_types::{
    get_code_key, get_known_code_key,
    utils::{key_for_eth_balance, storage_key_for_eth_balance},
    AccountTreeId, Address, Execute, StorageKey, H256, U256,
};
use zksync_utils::{
    bytecode::{hash_bytecode, hash_evm_bytecode},
    bytes_to_be_words, h256_to_u256,
};

use super::{default_system_env, TestedVm, VmTester, VmTesterBuilder};
use crate::interface::{
    storage::InMemoryStorage, TxExecutionMode, VmExecutionResultAndLogs, VmInterfaceExt,
};

const MOCK_DEPLOYER_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockContractDeployer.json";
const MOCK_KNOWN_CODE_STORAGE_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockKnownCodeStorage.json";
const MOCK_EMULATOR_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockEvmEmulator.json";
const RECURSIVE_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/NativeRecursiveContract.json";
const INCREMENTING_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/IncrementingContract.json";

fn override_system_contracts(storage: &mut InMemoryStorage) {
    let mock_deployer = read_bytecode(MOCK_DEPLOYER_PATH);
    let mock_deployer_hash = hash_bytecode(&mock_deployer);
    let mock_known_code_storage = read_bytecode(MOCK_KNOWN_CODE_STORAGE_PATH);
    let mock_known_code_storage_hash = hash_bytecode(&mock_known_code_storage);

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
            storage: InMemoryStorage::with_system_contracts(hash_bytecode),
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
        let mock_emulator = read_bytecode(MOCK_EMULATOR_PATH);
        let mut storage = self.storage;
        let mut system_env = default_system_env();
        if self.deploy_emulator {
            let evm_bytecode: Vec<_> = (0..32).collect();
            let evm_bytecode_hash = hash_evm_bytecode(&evm_bytecode);
            storage.set_value(
                get_known_code_key(&evm_bytecode_hash),
                H256::from_low_u64_be(1),
            );
            for evm_address in self.evm_contract_addresses {
                storage.set_value(get_code_key(&evm_address), evm_bytecode_hash);
            }

            system_env.base_system_smart_contracts.evm_emulator = Some(SystemContractCode {
                hash: hash_bytecode(&mock_emulator),
                code: bytes_to_be_words(mock_emulator),
            });
        } else {
            let emulator_hash = hash_bytecode(&mock_emulator);
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
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
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
    let expected_bytecode_hash = hash_evm_bytecode(&evm_bytecode);
    let execute = Execute::for_deploy(expected_bytecode_hash, vec![0; 32], &args);
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert_eq!(new_known_factory_deps.len(), 2); // the deployed EraVM contract + EVM contract
    assert_eq!(
        new_known_factory_deps[&expected_bytecode_hash],
        evm_bytecode
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
    let mock_emulator_abi = load_contract(MOCK_EMULATOR_PATH);
    let mut vm = EvmTestBuilder::new(deploy_emulator, RECIPIENT_ADDRESS).build::<VM>();

    let mut current_balance = U256::zero();
    for i in 1_u64..=5 {
        let transferred_value = (1_000_000_000 * i).into();
        let vm_result = test_payment(
            &mut vm,
            &mock_emulator_abi,
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
    let mock_emulator_abi = load_contract(MOCK_EMULATOR_PATH);
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
        vec![read_bytecode(RECURSIVE_CONTRACT_PATH)]
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
    let native_contract = read_bytecode(RECURSIVE_CONTRACT_PATH);
    let native_contract_abi = load_contract(RECURSIVE_CONTRACT_PATH);
    let deploy_tx = account.get_deploy_tx(
        &native_contract,
        Some(&[Token::Address(recipient_address)]),
        TxType::L2,
    );
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    // Call from the native contract to the EVM emulator.
    let test_fn = native_contract_abi.function("recurse").unwrap();
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

pub(crate) fn test_mock_emulator_with_deployment<VM: TestedVm>() {
    let contract_address = Address::repeat_byte(0xaa);
    let mut vm = EvmTestBuilder::new(true, contract_address)
        .with_mock_deployer()
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let mock_emulator_abi = load_contract(MOCK_EMULATOR_PATH);
    let new_evm_bytecode = vec![0xfe; 96];
    let new_evm_bytecode_hash = hash_evm_bytecode(&new_evm_bytecode);

    let test_fn = mock_emulator_abi.function("testDeploymentAndCall").unwrap();
    let test_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(contract_address),
            calldata: test_fn
                .encode_input(&[
                    Token::FixedBytes(new_evm_bytecode_hash.0.into()),
                    Token::Bytes(new_evm_bytecode.clone()),
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

    let factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert_eq!(
        factory_deps,
        HashMap::from([(new_evm_bytecode_hash, new_evm_bytecode)])
    );
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
    let native_contract = read_bytecode(INCREMENTING_CONTRACT_PATH);
    let native_contract_abi = load_contract(INCREMENTING_CONTRACT_PATH);
    let deploy_tx = account.get_deploy_tx(&native_contract, None, TxType::L2);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let test_fn = native_contract_abi.function("testDelegateCall").unwrap();
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
    let native_contract = read_bytecode(INCREMENTING_CONTRACT_PATH);
    let native_contract_abi = load_contract(INCREMENTING_CONTRACT_PATH);
    let deploy_tx = account.get_deploy_tx(&native_contract, None, TxType::L2);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    let test_fn = native_contract_abi.function("testStaticCall").unwrap();
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
