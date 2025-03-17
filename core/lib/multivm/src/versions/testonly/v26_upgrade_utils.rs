use std::collections::{HashMap, HashSet};

use ethabi::{Contract, Token};
use zksync_contracts::{l2_native_token_vault, load_sys_contract, read_l1_zk_contract};
use zksync_test_contracts::TestContract;
use zksync_types::{
    bytecode::BytecodeHash, protocol_upgrade::ProtocolUpgradeTxCommonData, u256_to_address,
    AccountTreeId, Address, Execute, ExecuteTransactionCommon, L1ChainId, L1TxCommonData,
    StorageKey, Transaction, COMPLEX_UPGRADER_ADDRESS, CONTRACT_FORCE_DEPLOYER_ADDRESS, H256,
    L1_MESSENGER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
    SYSTEM_CONTEXT_ADDRESS, U256,
};
use zksync_vm_interface::{
    storage::WriteStorage, InspectExecutionMode, TxExecutionMode, VmInterfaceExt,
};

use super::{TestedVm, VmTester};
use crate::{
    versions::testonly::{tester::TransactionTestInfo, ContractToDeploy, VmTesterBuilder},
    vm_latest::utils::v26_upgrade::{encode_legacy_finalize_deposit, get_test_data, V26TestData},
};

const SIMPLE_TEST_RESULT_JSON: &str = include_str!("./v26_utils_outputs/simple-test.json");
const POST_BRIDGING_TEST_RESULT_JSON: &str = include_str!("./v26_utils_outputs/post-bridging.json");
const POST_REGISTRATION_TEST_RESULT_JSON: &str =
    include_str!("./v26_utils_outputs/post-registration.json");

fn trivial_test_storage_logs() -> HashMap<StorageKey, H256> {
    let x: Vec<_> = serde_json::from_str(SIMPLE_TEST_RESULT_JSON).unwrap();
    x.into_iter().collect()
}

fn post_bridging_test_storage_logs() -> HashMap<StorageKey, H256> {
    let x: Vec<_> = serde_json::from_str(POST_BRIDGING_TEST_RESULT_JSON).unwrap();
    x.into_iter().collect()
}

fn post_registration_test_storage_logs() -> HashMap<StorageKey, H256> {
    let x: Vec<_> = serde_json::from_str(POST_REGISTRATION_TEST_RESULT_JSON).unwrap();
    x.into_iter().collect()
}

fn load_complex_upgrader_contract() -> Contract {
    load_sys_contract("ComplexUpgrader")
}

fn get_prepare_system_tx(
    l1_chain_id: L1ChainId,
    legacy_l1_token: Address,
    l1_shared_bridge: Address,
    test_contract_addr: Address,
    test_contract: &TestContract,
) -> Transaction {
    let beacon_proxy_bytecode = read_l1_zk_contract("BeaconProxy");

    let test_contract_calldata = test_contract
        .abi
        .function("resetLegacyParams")
        .unwrap()
        .encode_input(&[
            Token::Uint(l1_chain_id.0.into()),
            Token::Address(legacy_l1_token),
            Token::Address(l1_shared_bridge),
            Token::FixedBytes(
                BytecodeHash::for_bytecode(&beacon_proxy_bytecode)
                    .value()
                    .0
                    .to_vec(),
            ),
        ])
        .unwrap();

    let complex_upgrader = load_complex_upgrader_contract();
    let complex_upgrader_calldata = complex_upgrader
        .function("upgrade")
        .unwrap()
        .encode_input(&[
            Token::Address(test_contract_addr),
            Token::Bytes(test_contract_calldata),
        ])
        .unwrap();

    let mut dependencies: HashSet<Vec<u8>> = Default::default();
    for dep in test_contract.dependencies.iter() {
        dependencies.insert(dep.bytecode.to_vec());
    }

    let execute = Execute {
        contract_address: Some(COMPLEX_UPGRADER_ADDRESS),
        calldata: complex_upgrader_calldata,
        factory_deps: dependencies
            .into_iter()
            .chain(vec![beacon_proxy_bytecode])
            .collect(),
        value: U256::zero(),
    };

    Transaction {
        common_data: ExecuteTransactionCommon::ProtocolUpgrade(ProtocolUpgradeTxCommonData {
            sender: CONTRACT_FORCE_DEPLOYER_ADDRESS,
            gas_limit: U256::from(200_000_000u32),
            gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
            ..Default::default()
        }),
        execute,
        received_timestamp_ms: 0,
        raw_bytes: None,
    }
}

fn setup_v26_unsafe_deposits_detection<VM: TestedVm>() -> (VmTester<VM>, V26TestData) {
    // In this test, we compare the execution of the bootloader with the predefined
    // refunded gas and without them

    let l1_chain_id = L1ChainId(1);

    let test_contract = zksync_test_contracts::TestContract::bridge_test();
    // Any random (but big) address is fine
    let test_address = "abacabac00000000000000000000000000000000".parse().unwrap();
    // Any random (but big) address is fine
    let l1_shared_bridge_address = "abacabac00000000000000000000000000000001".parse().unwrap();
    // Any random (but big) address is fine
    let l1_token_address = "abacabac00000000000000000000000000000002".parse().unwrap();

    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(
            test_contract.bytecode.to_vec(),
            test_address,
        )])
        .with_rich_accounts(1)
        .build::<VM>();

    let system_tx = get_prepare_system_tx(
        l1_chain_id,
        l1_token_address,
        l1_shared_bridge_address,
        test_address,
        test_contract,
    );

    vm.vm.push_transaction(system_tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);

    assert!(!result.result.is_failed());

    let l2_token_address = vm.vm.read_storage(StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(0),
    ));
    let l2_legacy_shared_bridge_address = vm.vm.read_storage(StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(1),
    ));
    let l1_aliased_shared_bridge = vm.vm.read_storage(StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(2),
    ));

    let test_data = V26TestData {
        l1_chain_id,
        l1_shared_bridge_address,
        l1_token_address,
        l2_token_address: u256_to_address(&l2_token_address),
        l2_legacy_shared_bridge_address: u256_to_address(&l2_legacy_shared_bridge_address),
        l1_aliased_shared_bridge: u256_to_address(&l1_aliased_shared_bridge),
    };

    (vm, test_data)
}

fn encode_regisration(l2_token_address: Address) -> Vec<u8> {
    let contract = l2_native_token_vault();

    contract
        .function("setLegacyTokenAssetId")
        .unwrap()
        .encode_input(&[Token::Address(l2_token_address)])
        .unwrap()
}

// Returns a list of irrelevant keys that are not required to be the same.
// For some reason they do not consistently reproduce between test runs, but they are not relevant to the
// essence of the test.
fn get_irrelevant_keys() -> Vec<StorageKey> {
    vec![
        StorageKey::new(AccountTreeId::new(L1_MESSENGER_ADDRESS), H256::zero()),
        StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            H256::from_low_u64_be(10),
        ),
    ]
}

fn remove_irrelevant_keys(mut logs: HashMap<StorageKey, H256>) -> HashMap<StorageKey, H256> {
    for to_remove in get_irrelevant_keys() {
        logs.remove(&to_remove);
    }

    logs
}

pub(crate) fn test_trivial_test_storage_logs<VM: TestedVm>() {
    let (vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    assert_eq!(test_data, get_test_data());

    let storage_ptr = vm.storage.clone();
    let borrowed = storage_ptr.borrow();

    let expected = remove_irrelevant_keys(borrowed.modified_storage_keys().clone());
    let found = remove_irrelevant_keys(trivial_test_storage_logs());

    assert_eq!(expected, found);
}

pub(crate) fn test_post_bridging_test_storage_logs<VM: TestedVm>() {
    let (mut vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    assert_eq!(test_data, get_test_data());

    let l1_tx_new_deposit = Transaction {
        common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
            sender: test_data.l1_aliased_shared_bridge,
            gas_limit: 200_000_000u32.into(),
            gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
            ..Default::default()
        }),
        execute: Execute {
            contract_address: Some(test_data.l2_legacy_shared_bridge_address),
            calldata: encode_legacy_finalize_deposit(test_data.l1_token_address),
            value: Default::default(),
            factory_deps: vec![],
        },
        received_timestamp_ms: 0,
        raw_bytes: None,
    };

    vm.execute_tx_and_verify(TransactionTestInfo::new_processed(l1_tx_new_deposit, false));

    let storage_ptr = vm.storage.clone();
    let borrowed = storage_ptr.borrow();

    let expected = remove_irrelevant_keys(borrowed.modified_storage_keys().clone());
    let found: HashMap<StorageKey, H256> =
        remove_irrelevant_keys(post_bridging_test_storage_logs());

    assert_eq!(expected, found);
}

pub(crate) fn test_post_registration_storage_logs<VM: TestedVm>() {
    let (mut vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    assert_eq!(test_data, get_test_data());

    let l1_tx_regisrtation = Transaction {
        common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
            sender: test_data.l1_aliased_shared_bridge,
            gas_limit: 200_000_000u32.into(),
            gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
            ..Default::default()
        }),
        execute: Execute {
            contract_address: Some(L2_NATIVE_TOKEN_VAULT_ADDRESS),
            calldata: encode_regisration(test_data.l2_token_address),
            value: Default::default(),
            factory_deps: vec![],
        },
        received_timestamp_ms: 0,
        raw_bytes: None,
    };

    vm.execute_tx_and_verify(TransactionTestInfo::new_processed(
        l1_tx_regisrtation,
        false,
    ));

    let storage_ptr = vm.storage.clone();
    let borrowed = storage_ptr.borrow();

    let expected = remove_irrelevant_keys(borrowed.modified_storage_keys().clone());
    let found: HashMap<StorageKey, H256> =
        remove_irrelevant_keys(post_registration_test_storage_logs());

    assert_eq!(expected, found);
}
