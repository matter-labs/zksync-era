use std::collections::HashMap;

use ethabi::Token;
use test_casing::{test_casing, Product};
use zksync_contracts::{load_contract, read_bytecode, SystemContractCode};
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
};
use zksync_types::{
    get_code_key, get_known_code_key, utils::key_for_eth_balance, AccountTreeId, Address, Execute,
    StorageKey, H256, U256,
};
use zksync_utils::{be_words_to_bytes, bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256};
use zksync_vm_interface::VmInterfaceExt;

use crate::{
    interface::{storage::InMemoryStorage, TxExecutionMode},
    versions::testonly::default_system_env,
    vm_latest::{
        tests::tester::{VmTester, VmTesterBuilder},
        utils::hash_evm_bytecode,
        HistoryEnabled,
    },
};

const MOCK_DEPLOYER_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockContractDeployer.json";
const MOCK_KNOWN_CODE_STORAGE_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockKnownCodeStorage.json";
const MOCK_EMULATOR_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockEvmEmulator.json";
const RECURSIVE_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/NativeRecursiveContract.json";

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

#[test]
fn tracing_evm_contract_deployment() {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    override_system_contracts(&mut storage);

    let mut system_env = default_system_env();
    // The EVM emulator will not be accessed, so we set it to a dummy value.
    system_env.base_system_smart_contracts.evm_emulator =
        Some(system_env.base_system_smart_contracts.default_aa.clone());
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();
    let account = &mut vm.rich_accounts[0];

    let args = [Token::Bytes((0..=u8::MAX).collect())];
    let evm_bytecode = ethabi::encode(&args);
    let expected_bytecode_hash = hash_evm_bytecode(&evm_bytecode);
    let execute = Execute::for_deploy(expected_bytecode_hash, vec![0; 32], &args);
    let deploy_tx = account.get_l2_tx_for_execute(execute, None);
    let (_, vm_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx, true);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);

    // Check that the surrogate EVM bytecode was added to the decommitter.
    let known_bytecodes = vm.vm.state.decommittment_processor.known_bytecodes.inner();
    let known_evm_bytecode =
        be_words_to_bytes(&known_bytecodes[&h256_to_u256(expected_bytecode_hash)]);
    assert_eq!(known_evm_bytecode, evm_bytecode);

    let new_known_factory_deps = vm_result.new_known_factory_deps.unwrap();
    assert_eq!(new_known_factory_deps.len(), 2); // the deployed EraVM contract + EVM contract
    assert_eq!(
        new_known_factory_deps[&expected_bytecode_hash],
        evm_bytecode
    );
}

#[test]
fn mock_emulator_basics() {
    let mock_emulator = read_bytecode(MOCK_EMULATOR_PATH);
    let called_address = Address::repeat_byte(0x23);
    let evm_bytecode: Vec<_> = (0..=u8::MAX).collect();
    let evm_bytecode_hash = hash_evm_bytecode(&evm_bytecode);

    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    storage.set_value(get_code_key(&called_address), evm_bytecode_hash);
    storage.set_value(
        get_known_code_key(&evm_bytecode_hash),
        H256::from_low_u64_be(1),
    );
    let mut system_env = default_system_env();
    system_env.base_system_smart_contracts.evm_emulator = Some(SystemContractCode {
        hash: hash_bytecode(&mock_emulator),
        code: bytes_to_be_words(mock_emulator),
    });

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();
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

fn build_vm(deploy_emulator: bool, contract_address: Address) -> VmTester<HistoryEnabled> {
    let mock_emulator = read_bytecode(MOCK_EMULATOR_PATH);
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    let mut system_env = default_system_env();
    if deploy_emulator {
        let evm_bytecode: Vec<_> = (0..=u8::MAX).collect();
        let evm_bytecode_hash = hash_evm_bytecode(&evm_bytecode);
        storage.set_value(get_code_key(&contract_address), evm_bytecode_hash);
        storage.set_value(
            get_known_code_key(&evm_bytecode_hash),
            H256::from_low_u64_be(1),
        );

        system_env.base_system_smart_contracts.evm_emulator = Some(SystemContractCode {
            hash: hash_bytecode(&mock_emulator),
            code: bytes_to_be_words(mock_emulator),
        });
    } else {
        let emulator_hash = hash_bytecode(&mock_emulator);
        storage.set_value(get_code_key(&contract_address), emulator_hash);
        storage.set_value(get_known_code_key(&emulator_hash), H256::from_low_u64_be(1));
        storage.store_factory_dep(emulator_hash, mock_emulator);
        // Set `isUserSpace` in the emulator storage to `true`, so that it skips emulator-specific checks
        storage.set_value(
            StorageKey::new(AccountTreeId::new(contract_address), H256::zero()),
            H256::from_low_u64_be(1),
        );
    }

    VmTesterBuilder::new(HistoryEnabled)
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build()
}

/// `deploy_emulator = false` here and below tests the mock emulator as an ordinary contract (i.e., sanity-checks its logic).
#[test_casing(2, [false, true])]
#[test]
fn mock_emulator_with_payment(deploy_emulator: bool) {
    let mock_emulator_abi = load_contract(MOCK_EMULATOR_PATH);
    let recipient_address = Address::repeat_byte(0x12);
    let mut vm = build_vm(deploy_emulator, recipient_address);
    let account = &mut vm.rich_accounts[0];

    let test_payment_fn = mock_emulator_abi.function("testPayment").unwrap();

    let mut current_balance = U256::zero();
    for i in 1_u64..=5 {
        let transferred_value = (1_000_000_000 * i).into();
        current_balance += transferred_value;
        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(recipient_address),
                calldata: test_payment_fn
                    .encode_input(&[Token::Uint(transferred_value), Token::Uint(current_balance)])
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

        let balance_storage_logs = vm_result.logs.storage_logs.iter().filter_map(|log| {
            (*log.log.key.address() == L2_BASE_TOKEN_ADDRESS)
                .then_some((*log.log.key.key(), h256_to_u256(log.log.value)))
        });
        let balances: HashMap<_, _> = balance_storage_logs.collect();
        assert_eq!(
            balances[&key_for_eth_balance(&recipient_address)],
            current_balance
        );
    }
}

#[test_casing(4, Product(([false, true], [false, true])))]
#[test]
fn mock_emulator_with_recursion(deploy_emulator: bool, is_external: bool) {
    let mock_emulator_abi = load_contract(MOCK_EMULATOR_PATH);
    let recipient_address = Address::repeat_byte(0x12);
    let mut vm = build_vm(deploy_emulator, recipient_address);
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

#[test]
fn mock_emulator_with_deployment() {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    override_system_contracts(&mut storage);

    let mut system_env = default_system_env();
    let evm_bytecode: Vec<_> = (0..=u8::MAX).collect();
    let evm_bytecode_hash = hash_evm_bytecode(&evm_bytecode);
    let contract_address = Address::repeat_byte(0xaa);
    storage.set_value(get_code_key(&contract_address), evm_bytecode_hash);
    storage.set_value(
        get_known_code_key(&evm_bytecode_hash),
        H256::from_low_u64_be(1),
    );

    let mock_emulator = read_bytecode(MOCK_EMULATOR_PATH);
    system_env.base_system_smart_contracts.evm_emulator = Some(SystemContractCode {
        hash: hash_bytecode(&mock_emulator),
        code: bytes_to_be_words(mock_emulator),
    });

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_system_env(system_env)
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();
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
