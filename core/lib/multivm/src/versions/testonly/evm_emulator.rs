use std::{collections::HashMap, iter};

use assert_matches::assert_matches;
use ethabi::Token;
use zksync_contracts::{
    load_contract, read_bytecode, read_deployed_bytecode_from_path, SystemContractCode,
};
use zksync_system_constants::{
    CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS, L2_BASE_TOKEN_ADDRESS,
};
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher,
    get_code_key, get_deployer_key, get_evm_code_hash_key, get_known_code_key,
    system_contracts::get_system_smart_contracts,
    utils::{key_for_eth_balance, storage_key_for_eth_balance},
    web3, AccountTreeId, Address, Execute, L2BlockNumber, ProtocolVersionId, StorageKey, H256,
    U256,
};
use zksync_utils::{
    bytecode::{hash_bytecode, hash_evm_bytecode},
    bytes_to_be_words, h256_to_u256,
};

use super::{default_system_env, TestedVm, VmTester, VmTesterBuilder};
use crate::{
    interface::{
        storage::InMemoryStorage, ExecutionResult, L2BlockEnv, TxExecutionMode,
        VmExecutionResultAndLogs, VmInterfaceExt, VmRevertReason,
    },
    utils::get_batch_base_fee,
};

const MOCK_DEPLOYER_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockContractDeployer.json";
const MOCK_KNOWN_CODE_STORAGE_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockKnownCodeStorage.json";
const MOCK_EMULATOR_PATH: &str =
    "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/MockEvmEmulator.json";
const RECURSIVE_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/NativeRecursiveContract.json";
const INCREMENTING_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts-zk/contracts/mock-evm/mock-evm.sol/IncrementingContract.json";

const EVM_TEST_CONTRACT_PATH: &str =
    "etc/contracts-test-data/artifacts/evm.sol/EvmEmulationTest.json";
//const EVM_RECURSIVE_CONTRACT_PATH: &str = "etc/contracts-test-data/artifacts/evm.sol/EvmRecursiveContract.json";

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

const EVM_ADDRESS: Address = Address::repeat_byte(1);

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
