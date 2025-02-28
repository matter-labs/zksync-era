use std::str::FromStr;

use ethabi::{Contract, Token};
use zksync_contracts::{
    l2_native_token_vault, load_l1_zk_contract, load_sys_contract, read_l1_zk_contract,
};
use zksync_types::{
    bytecode::BytecodeHash, h256_to_address, protocol_upgrade::ProtocolUpgradeTxCommonData,
    AccountTreeId, Address, Execute, ExecuteTransactionCommon, L1TxCommonData, StorageKey,
    Transaction, COMPLEX_UPGRADER_ADDRESS, CONTRACT_FORCE_DEPLOYER_ADDRESS, H256,
    L2_NATIVE_TOKEN_VAULT_ADDRESS, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256,
};
use zksync_vm_interface::{
    storage::{ReadStorage, WriteStorage},
    InspectExecutionMode, TxExecutionMode, VmInterfaceExt,
};

use super::{TestedVm, VmTester};
use crate::{
    versions::testonly::{tester::TransactionTestInfo, ContractToDeploy, VmTesterBuilder},
    vm_latest::utils::v26_upgrade::{
        encode_legacy_finalize_deposit, get_test_data, post_bridging_test_storage_logs,
        post_registration_test_storage_logs, trivial_test_storage_logs, V26TestData,
    },
};

fn load_complex_upgrader_contract() -> Contract {
    load_sys_contract("ComplexUpgrader")
}

fn get_prepare_system_tx(
    l1_chain_id: U256,
    legacy_l1_token: Address,
    l1_shared_bridge: Address,
    test_contract_addr: Address,
) -> Transaction {
    let beacon_proxy_bytecode = read_l1_zk_contract("BeaconProxy");

    let test_contract = load_l1_zk_contract("LegacySharedBridgeTest");

    let test_contract_calldata = test_contract
        .function("resetLegacyParams")
        .unwrap()
        .encode_input(&[
            Token::Uint(l1_chain_id),
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

    let execute = Execute {
        contract_address: Some(COMPLEX_UPGRADER_ADDRESS),
        calldata: complex_upgrader_calldata,
        factory_deps: vec![
            beacon_proxy_bytecode,
            // There are also some more auxilary factory deps needed
            // to make the transaction work
            read_l1_zk_contract("L2SharedBridgeV25"),
            read_l1_zk_contract("TransparentUpgradeableProxy"),
            read_l1_zk_contract("L2StandardERC20V25"),
            read_l1_zk_contract("UpgradeableBeacon"),
            read_l1_zk_contract("L2SharedBridgeLegacy"),
            read_l1_zk_contract("ProxyAdmin"),
            read_l1_zk_contract("BridgedStandardERC20"),
        ],
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

    let l1_chain_id = U256::from(1u32);

    let test_bytecode = read_l1_zk_contract("LegacySharedBridgeTest");
    // Any random (but big) address is fine
    let test_address = Address::from_str("abacabac00000000000000000000000000000000").unwrap();
    // Any random (but big) address is fine
    let l1_shared_bridge_address =
        Address::from_str("abacabac00000000000000000000000000000001").unwrap();
    // Any random (but big) address is fine
    let l1_token_address = Address::from_str("abacabac00000000000000000000000000000002").unwrap();

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(test_bytecode, test_address)])
        .with_rich_accounts(1)
        .build::<VM>();

    let system_tx = get_prepare_system_tx(
        l1_chain_id,
        l1_token_address,
        l1_shared_bridge_address,
        test_address,
    );

    vm.vm.push_transaction(system_tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);

    assert!(!result.result.is_failed());

    println!("{:#?}", H256::from_low_u64_be(1));
    let l2_token_address = vm.storage.borrow_mut().read_value(&StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(0),
    ));
    let l2_legacy_shared_bridge_address = vm.storage.borrow_mut().read_value(&StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(1),
    ));
    let l1_aliased_shared_bridge = vm.storage.borrow_mut().read_value(&StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(2),
    ));

    let test_data = V26TestData {
        l1_chain_id,
        l1_shared_bridge_address,
        l1_token_address,
        l2_token_address: h256_to_address(&l2_token_address),
        l2_legacy_shared_bridge_address: h256_to_address(&l2_legacy_shared_bridge_address),
        l1_aliased_shared_bridge: h256_to_address(&l1_aliased_shared_bridge),
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

pub(crate) fn test_trivial_test_storage_logs<VM: TestedVm>() {
    let (vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    assert_eq!(test_data, get_test_data());

    let storage_ptr = vm.storage.clone();
    let borrowed = storage_ptr.borrow();

    let tmp: Vec<_> = borrowed
        .modified_storage_keys()
        .clone()
        .into_iter()
        .collect();
    println!("{}", serde_json::to_string(&tmp).unwrap());

    assert_eq!(
        borrowed.modified_storage_keys().clone(),
        trivial_test_storage_logs()
    );
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

    assert_eq!(
        borrowed.modified_storage_keys().clone(),
        post_bridging_test_storage_logs()
    );
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

    assert_eq!(
        borrowed.modified_storage_keys().clone(),
        post_registration_test_storage_logs()
    );
}

pub(crate) async fn test_v26_unsafe_deposit_detection_trivial<VM: TestedVm>() {
    let (mut vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();

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

    let keys: Vec<_> = borrowed
        .modified_storage_keys()
        .clone()
        .into_iter()
        .collect();
    println!("{}", serde_json::to_string(&keys).unwrap());

    // assert_eq!(borrowed.modified_storage_keys().clone(), trivial_test_storage_logs());

    drop(vm);
    drop(borrowed);
    let inner = std::rc::Rc::try_unwrap(storage_ptr).unwrap().into_inner();

    // // No transactions, obviously no bad deposits.
    // assert_eq!(
    //     is_unsafe_deposit_present(&[], &mut inner).await.unwrap(),
    //     false
    // );

    // let l2_tx = Transaction {
    //     common_data: ExecuteTransactionCommon::L2(Default::default()),
    //     execute: Execute {
    //         contract_address: Some(test_data.l2_legacy_shared_bridge_address),
    //         calldata: encode_legacy_finalize_deposit(test_data.l1_token_address),
    //         value: Default::default(),
    //         factory_deps: vec![],
    //     },
    //     received_timestamp_ms: 0,
    //     raw_bytes: None,
    // };

    // let l1_tx = Transaction {
    //     common_data: ExecuteTransactionCommon::L1(Default::default()),
    //     ..l2_tx.clone()
    // };

    // let l1_tx_new_deposit = Transaction {
    //     common_data: ExecuteTransactionCommon::L1(Default::default()),
    //     execute: Execute {
    //         contract_address: Some(L2_ASSET_ROUTER_ADDRESS),
    //         calldata: encode_new_finalize_deposit(
    //             test_data.l1_chain_id,
    //             test_data.l1_token_address,
    //         ),
    //         value: Default::default(),
    //         factory_deps: vec![],
    //     },
    //     ..l2_tx.clone()
    // };

    // let l1_tx_new_deposit_bad_address = Transaction {
    //     common_data: ExecuteTransactionCommon::L1(Default::default()),
    //     execute: Execute {
    //         contract_address: Some(Address::from_low_u64_be(1)),
    //         calldata: encode_new_finalize_deposit(
    //             test_data.l1_chain_id,
    //             test_data.l1_token_address,
    //         ),
    //         value: Default::default(),
    //         factory_deps: vec![],
    //     },
    //     ..l2_tx.clone()
    // };

    // // Even though the transaction could've been a legacy one, it is still accepted as it is an L2 one.
    // assert_eq!(
    //     is_unsafe_deposit_present(&[(l2_tx.clone(), Default::default())], &mut inner)
    //         .await
    //         .unwrap(),
    //     false
    // );

    // // The second transaction is a legacy one and thus it should be accepted.
    // assert_eq!(
    //     is_unsafe_deposit_present(
    //         &[(l2_tx, Default::default()), (l1_tx, Default::default())],
    //         &mut inner
    //     )
    //     .await
    //     .unwrap(),
    //     false
    // );

    // // The second transaction is a legacy one and thus it should be accepted.
    // assert_eq!(
    //     is_unsafe_deposit_present(&[(l1_tx_new_deposit, Default::default())], &mut inner)
    //         .await
    //         .unwrap(),
    //     false
    // );

    // // The second transaction is a legacy one and thus it should be accepted.
    // assert_eq!(
    //     is_unsafe_deposit_present(
    //         &[(l1_tx_new_deposit_bad_address, Default::default())],
    //         &mut inner
    //     )
    //     .await
    //     .unwrap(),
    //     false
    // );
}

// #[async_trait::async_trait]
// impl AsyncStorageKeyAccess for StorageView<InMemoryStorage> {
//     async fn read_key(&mut self, key: &StorageKey) -> anyhow::Result<H256> {
//         Ok(self.read_value(key))
//     }
// }
