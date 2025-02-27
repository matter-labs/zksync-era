use ethabi::{Contract, Token};
use tokio::runtime::Runtime;
use zksync_contracts::{l2_asset_router, l2_legacy_shared_bridge, load_contract, load_l1_zk_contract, load_sys_contract, read_bytecode, read_l1_zk_contract};
use zksync_test_contracts::Account;
use zksync_types::{bytecode::BytecodeHash, h256_to_address, protocol_upgrade::ProtocolUpgradeTxCommonData, AccountTreeId, Address, Execute, ExecuteTransactionCommon, StorageKey, Transaction, COMPLEX_UPGRADER_ADDRESS, CONTRACT_FORCE_DEPLOYER_ADDRESS, H256, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256};
use zksync_vm_interface::{storage::{InMemoryStorage, ReadStorage, StorageView}, InspectExecutionMode, TxExecutionMode, VmInterfaceExt};
use std::str::FromStr;
use crate::{versions::testonly::{ContractToDeploy, VmTesterBuilder}, vm_latest::utils::v26_upgrade::{is_unsafe_deposit_present, AsyncStorageKeyAccess}};

use super::{TestedVm, VmTester};

fn load_complex_upgrader_contract() -> Contract {
    load_sys_contract("ComplexUpgrader")
}

fn get_prepare_system_tx(
    legacy_l1_token: Address,
    l1_shared_bridge: Address,
    test_contract_addr: Address,
) -> Transaction {
    let beacon_proxy_bytecode = read_l1_zk_contract("BeaconProxy");

    let test_contract = load_l1_zk_contract("LegacySharedBridgeTest");

    let test_contract_calldata = test_contract.function("resetLegacyParams").unwrap().encode_input(&[
        Token::Address(legacy_l1_token),
        Token::Address(l1_shared_bridge),
        Token::FixedBytes(BytecodeHash::for_bytecode(&beacon_proxy_bytecode).value().0.to_vec())
    ]).unwrap();

    let complex_upgrader = load_complex_upgrader_contract();
    let complex_upgrader_calldata = complex_upgrader.function("upgrade").unwrap().encode_input(&[
        Token::Address(test_contract_addr),
        Token::Bytes(test_contract_calldata)
    ]).unwrap();

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
            read_l1_zk_contract("ProxyAdmin")
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

#[derive(Debug)]
struct TestData {
    l1_shared_bridge_address: Address,
    l1_token_address: Address,
    l2_token_address: Address,
    l2_legacy_shared_bridge_address: Address
}

fn setup_v26_unsafe_deposits_detection<VM: TestedVm>() -> (VmTester<VM>, TestData) {
    // In this test, we compare the execution of the bootloader with the predefined
    // refunded gas and without them

    let test_bytecode = read_l1_zk_contract("LegacySharedBridgeTest");
    // Any random (but big) address is fine
    let test_address = Address::from_str("abacabac00000000000000000000000000000000").unwrap();
    // Any random (but big) address is fine
    let l1_shared_bridge_address = Address::from_str("abacabac00000000000000000000000000000001").unwrap();
    // Any random (but big) address is fine
    let l1_token_address = Address::from_str("abacabac00000000000000000000000000000002").unwrap();


    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![
            ContractToDeploy::new(test_bytecode, test_address)
        ])
        .with_rich_accounts(1)
        .build::<VM>();

    let system_tx = get_prepare_system_tx(
        l1_token_address, 
        l1_shared_bridge_address,
        test_address
    );

    vm.vm.push_transaction(system_tx);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);

    assert!(!result.result.is_failed());

    println!("{:#?}", H256::from_low_u64_be(1));
    let l2_token_address = vm.storage.borrow_mut().read_value(&StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(0)
    ));
    let l2_legacy_shared_bridge_address = vm.storage.borrow_mut().read_value(&StorageKey::new(
        AccountTreeId::new(COMPLEX_UPGRADER_ADDRESS),
        H256::from_low_u64_be(1)
    ));

    let test_data = TestData {
        l1_shared_bridge_address,
        l1_token_address,
        l2_token_address: h256_to_address(&l2_token_address),
        l2_legacy_shared_bridge_address: h256_to_address(&l2_legacy_shared_bridge_address) 
    };

    (vm, test_data)
}

fn empty_erc20_metadata() -> Vec<u8> {
    ethabi::encode(&[
        Token::Bytes(vec![]),
        Token::Bytes(vec![]),
        Token::Bytes(vec![])
    ])
}

fn encode_legacy_finalize_deposit(l1_token_address: Address) -> Vec<u8> {
    let legacy_shared_bridge = l2_legacy_shared_bridge();
    legacy_shared_bridge.function("finalizeDeposit").unwrap().encode_input(&[
        Token::Address(Address::from_low_u64_be(1)),
        Token::Address(Address::from_low_u64_be(2)),
        Token::Address(l1_token_address),
        Token::Uint(U256::from(1)),
        Token::Bytes(empty_erc20_metadata())
    ]).unwrap()
}

fn encode_new_finalize_deposit() -> Vec<u8> {
    todo!()
}

pub(crate) async fn test_v26_unsafe_deposit_detection_trivial<VM: TestedVm>() {
    let (vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();

    let storage_ptr = vm.storage.clone();
    drop(vm);
    let mut inner = std::rc::Rc::try_unwrap(storage_ptr).unwrap().into_inner();

    // No transactions, obviously no bad deposits.
    assert_eq!(is_unsafe_deposit_present(
        &[],
        &mut inner
    ).await.unwrap(), false);

    let l2_tx = Transaction {
        common_data: ExecuteTransactionCommon::L2(Default::default()),
        execute: Execute { 
            contract_address: Some(test_data.l2_legacy_shared_bridge_address), 
            calldata: encode_legacy_finalize_deposit(test_data.l1_token_address), 
            value: Default::default(), 
            factory_deps: vec![] 
        },
        received_timestamp_ms: 0,
        raw_bytes: None
    };

    let l1_tx = Transaction {
        common_data: ExecuteTransactionCommon::L1(Default::default()),
        ..l2_tx.clone()
    };

    // Even though the transaction could've been a legacy one, it is still accepted as it is an L2 one.
    assert_eq!(is_unsafe_deposit_present(
        &[(l2_tx.clone(), Default::default())],
        &mut inner
    ).await.unwrap(), false);

    // The second transaction is a legacy one and thus it should be accepted.
    assert_eq!(is_unsafe_deposit_present(
        &[
            (l2_tx, Default::default()),
            (l1_tx, Default::default())
        ],
        &mut inner
    ).await.unwrap(), true);
}

#[async_trait::async_trait]
impl AsyncStorageKeyAccess for StorageView<InMemoryStorage> {
    async fn read_key(&mut self, key: &StorageKey) -> anyhow::Result<H256> {
        Ok(self.read_value(key))
    }
}
