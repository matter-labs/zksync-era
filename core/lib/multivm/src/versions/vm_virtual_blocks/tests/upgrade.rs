use zk_evm_1_3_3::aux_structures::Timestamp;

use zksync_types::{
    ethabi::Contract,
    Execute, COMPLEX_UPGRADER_ADDRESS, CONTRACT_DEPLOYER_ADDRESS, CONTRACT_FORCE_DEPLOYER_ADDRESS,
    REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE,
    {ethabi::Token, Address, ExecuteTransactionCommon, Transaction, H256, U256},
    {get_code_key, get_known_code_key, H160},
};

use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};

use zksync_contracts::{deployer_contract, load_contract, load_sys_contract, read_bytecode};
use zksync_state::WriteStorage;
use zksync_test_account::TxType;

use crate::interface::{
    ExecutionResult, Halt, TxExecutionMode, VmExecutionMode, VmInterface, VmInterfaceHistoryEnabled,
};
use crate::vm_latest::HistoryEnabled;
use crate::vm_virtual_blocks::tests::tester::VmTesterBuilder;
use crate::vm_virtual_blocks::tests::utils::verify_required_storage;
use zksync_types::protocol_version::ProtocolUpgradeTxCommonData;

use super::utils::read_test_contract;

/// In this test we ensure that the requirements for protocol upgrade transactions are enforced by the bootloader:
/// - This transaction must be the only one in block
/// - If present, this transaction must be the first one in block
#[test]
fn test_protocol_upgrade_is_first() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let bytecode_hash = hash_bytecode(&read_test_contract());

    // Here we just use some random transaction of protocol upgrade type:
    let protocol_upgrade_transaction = get_forced_deploy_tx(&[ForceDeployment {
        // The bytecode hash to put on an address
        bytecode_hash,
        // The address on which to deploy the bytecodehash to
        address: H160::random(),
        // Whether to run the constructor on the force deployment
        call_constructor: false,
        // The value with which to initialize a contract
        value: U256::zero(),
        // The constructor calldata
        input: vec![],
    }]);

    let normal_l1_transaction = vm.rich_accounts[0]
        .get_deploy_tx(&read_test_contract(), None, TxType::L1 { serial_id: 0 })
        .tx;

    let expected_error =
        Halt::UnexpectedVMBehavior("Assertion error: Protocol upgrade tx not first".to_string());

    vm.vm.make_snapshot();
    // Test 1: there must be only one system transaction in block
    vm.vm.push_transaction(protocol_upgrade_transaction.clone());
    vm.vm.push_transaction(normal_l1_transaction.clone());
    vm.vm.push_transaction(protocol_upgrade_transaction.clone());

    vm.vm.execute(VmExecutionMode::OneTx);
    vm.vm.execute(VmExecutionMode::OneTx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert_eq!(
        result.result,
        ExecutionResult::Halt {
            reason: expected_error.clone()
        }
    );

    // Test 2: the protocol upgrade tx must be the first one in block
    vm.vm.rollback_to_the_latest_snapshot();
    vm.vm.make_snapshot();
    vm.vm.push_transaction(normal_l1_transaction.clone());
    vm.vm.push_transaction(protocol_upgrade_transaction.clone());

    vm.vm.execute(VmExecutionMode::OneTx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert_eq!(
        result.result,
        ExecutionResult::Halt {
            reason: expected_error
        }
    );

    vm.vm.rollback_to_the_latest_snapshot();
    vm.vm.make_snapshot();
    vm.vm.push_transaction(protocol_upgrade_transaction);
    vm.vm.push_transaction(normal_l1_transaction);

    vm.vm.execute(VmExecutionMode::OneTx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed());
}

/// In this test we try to test how force deployments could be done via protocol upgrade transactions.
#[test]
fn test_force_deploy_upgrade() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let storage_view = vm.storage.clone();
    let bytecode_hash = hash_bytecode(&read_test_contract());

    let known_code_key = get_known_code_key(&bytecode_hash);
    // It is generally expected that all the keys will be set as known prior to the protocol upgrade.
    storage_view
        .borrow_mut()
        .set_value(known_code_key, u256_to_h256(1.into()));
    drop(storage_view);

    let address_to_deploy = H160::random();
    // Here we just use some random transaction of protocol upgrade type:
    let transaction = get_forced_deploy_tx(&[ForceDeployment {
        // The bytecode hash to put on an address
        bytecode_hash,
        // The address on which to deploy the bytecodehash to
        address: address_to_deploy,
        // Whether to run the constructor on the force deployment
        call_constructor: false,
        // The value with which to initialize a contract
        value: U256::zero(),
        // The constructor calldata
        input: vec![],
    }]);

    vm.vm.push_transaction(transaction);

    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "The force upgrade was not successful"
    );

    let expected_slots = vec![(bytecode_hash, get_code_key(&address_to_deploy))];

    // Verify that the bytecode has been set correctly
    verify_required_storage(&vm.vm.state, expected_slots);
}

/// Here we show how the work with the complex upgrader could be done
#[test]
fn test_complex_upgrader() {
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let storage_view = vm.storage.clone();

    let bytecode_hash = hash_bytecode(&read_complex_upgrade());
    let msg_sender_test_hash = hash_bytecode(&read_msg_sender_test());

    // Let's assume that the bytecode for the implementation of the complex upgrade
    // is already deployed in some address in userspace
    let upgrade_impl = H160::random();
    let account_code_key = get_code_key(&upgrade_impl);

    storage_view
        .borrow_mut()
        .set_value(get_known_code_key(&bytecode_hash), u256_to_h256(1.into()));
    storage_view.borrow_mut().set_value(
        get_known_code_key(&msg_sender_test_hash),
        u256_to_h256(1.into()),
    );
    storage_view
        .borrow_mut()
        .set_value(account_code_key, bytecode_hash);
    drop(storage_view);

    vm.vm.state.decommittment_processor.populate(
        vec![
            (
                h256_to_u256(bytecode_hash),
                bytes_to_be_words(read_complex_upgrade()),
            ),
            (
                h256_to_u256(msg_sender_test_hash),
                bytes_to_be_words(read_msg_sender_test()),
            ),
        ],
        Timestamp(0),
    );

    let address_to_deploy1 = H160::random();
    let address_to_deploy2 = H160::random();

    let transaction = get_complex_upgrade_tx(
        upgrade_impl,
        address_to_deploy1,
        address_to_deploy2,
        bytecode_hash,
    );

    vm.vm.push_transaction(transaction);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "The force upgrade was not successful"
    );

    let expected_slots = vec![
        (bytecode_hash, get_code_key(&address_to_deploy1)),
        (bytecode_hash, get_code_key(&address_to_deploy2)),
    ];

    // Verify that the bytecode has been set correctly
    verify_required_storage(&vm.vm.state, expected_slots);
}

#[derive(Debug, Clone)]
struct ForceDeployment {
    // The bytecode hash to put on an address
    bytecode_hash: H256,
    // The address on which to deploy the bytecodehash to
    address: Address,
    // Whether to run the constructor on the force deployment
    call_constructor: bool,
    // The value with which to initialize a contract
    value: U256,
    // The constructor calldata
    input: Vec<u8>,
}

fn get_forced_deploy_tx(deployment: &[ForceDeployment]) -> Transaction {
    let deployer = deployer_contract();
    let contract_function = deployer.function("forceDeployOnAddresses").unwrap();

    let encoded_deployments: Vec<_> = deployment
        .iter()
        .map(|deployment| {
            Token::Tuple(vec![
                Token::FixedBytes(deployment.bytecode_hash.as_bytes().to_vec()),
                Token::Address(deployment.address),
                Token::Bool(deployment.call_constructor),
                Token::Uint(deployment.value),
                Token::Bytes(deployment.input.clone()),
            ])
        })
        .collect();

    let params = [Token::Array(encoded_deployments)];

    let calldata = contract_function
        .encode_input(&params)
        .expect("failed to encode parameters");

    let execute = Execute {
        contract_address: CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        factory_deps: None,
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

// Returns the transaction that performs a complex protocol upgrade.
// The first param is the address of the implementation of the complex upgrade
// in user-space, while the next 3 params are params of the implenentaiton itself
// For the explanatation for the parameters, please refer to:
// etc/contracts-test-data/complex-upgrade/complex-upgrade.sol
fn get_complex_upgrade_tx(
    implementation_address: Address,
    address1: Address,
    address2: Address,
    bytecode_hash: H256,
) -> Transaction {
    let impl_contract = get_complex_upgrade_abi();
    let impl_function = impl_contract.function("someComplexUpgrade").unwrap();
    let impl_calldata = impl_function
        .encode_input(&[
            Token::Address(address1),
            Token::Address(address2),
            Token::FixedBytes(bytecode_hash.as_bytes().to_vec()),
        ])
        .unwrap();

    let complex_upgrader = get_complex_upgrader_abi();
    let upgrade_function = complex_upgrader.function("upgrade").unwrap();
    let complex_upgrader_calldata = upgrade_function
        .encode_input(&[
            Token::Address(implementation_address),
            Token::Bytes(impl_calldata),
        ])
        .unwrap();

    let execute = Execute {
        contract_address: COMPLEX_UPGRADER_ADDRESS,
        calldata: complex_upgrader_calldata,
        factory_deps: None,
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

fn read_complex_upgrade() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json")
}

fn read_msg_sender_test() -> Vec<u8> {
    read_bytecode("etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/msg-sender.sol/MsgSenderTest.json")
}

fn get_complex_upgrade_abi() -> Contract {
    load_contract(
        "etc/contracts-test-data/artifacts-zk/contracts/complex-upgrade/complex-upgrade.sol/ComplexUpgrade.json"
    )
}

fn get_complex_upgrader_abi() -> Contract {
    load_sys_contract("ComplexUpgrader")
}
