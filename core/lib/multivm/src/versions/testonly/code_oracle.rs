use ethabi::Token;
use zksync_test_contracts::TestContract;
use zksync_types::{
    bytecode::BytecodeHash, get_known_code_key, h256_to_u256, u256_to_h256, web3::keccak256,
    Address, Execute, StorageLogWithPreviousValue, U256,
};

use super::{get_empty_storage, tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterfaceExt},
    versions::testonly::ContractToDeploy,
};

fn generate_large_bytecode() -> Vec<u8> {
    // This is the maximal possible size of a zkEVM bytecode
    vec![2u8; ((1 << 16) - 1) * 32]
}

pub(crate) fn test_code_oracle<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);
    let precompile_contract_bytecode = TestContract::precompiles_test().bytecode.to_vec();

    // Filling the zkevm bytecode
    let normal_zkevm_bytecode = TestContract::counter().bytecode;
    let normal_zkevm_bytecode_hash = BytecodeHash::for_bytecode(normal_zkevm_bytecode).value();
    let normal_zkevm_bytecode_keccak_hash = keccak256(normal_zkevm_bytecode);
    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&normal_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            precompile_contract_bytecode,
            precompiles_contract_address,
        )])
        .with_storage(storage)
        .build::<VM>();

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    vm.vm.insert_bytecodes(&[normal_zkevm_bytecode]);
    let account = &mut vm.rich_accounts[0];

    // Firstly, let's ensure that the contract works.
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(precompiles_contract_address),
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(normal_zkevm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(normal_zkevm_bytecode_keccak_hash.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx1);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );

    // Now, we ask for the same bytecode. We use to partially check whether the memory page with
    // the decommitted bytecode gets erased (it shouldn't).
    let tx2 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(precompiles_contract_address),
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(normal_zkevm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(normal_zkevm_bytecode_keccak_hash.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx2);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );
}

fn find_code_oracle_cost_log(
    precompiles_contract_address: Address,
    logs: &[StorageLogWithPreviousValue],
) -> &StorageLogWithPreviousValue {
    logs.iter()
        .find(|log| {
            *log.log.key.address() == precompiles_contract_address && log.log.key.key().is_zero()
        })
        .expect("no code oracle cost log")
}

pub(crate) fn test_code_oracle_big_bytecode<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);
    let precompile_contract_bytecode = TestContract::precompiles_test().bytecode.to_vec();

    let big_zkevm_bytecode = generate_large_bytecode();
    let big_zkevm_bytecode_hash = BytecodeHash::for_bytecode(&big_zkevm_bytecode).value();
    let big_zkevm_bytecode_keccak_hash = keccak256(&big_zkevm_bytecode);

    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&big_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            precompile_contract_bytecode,
            precompiles_contract_address,
        )])
        .with_storage(storage)
        .build::<VM>();

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    vm.vm.insert_bytecodes(&[big_zkevm_bytecode.as_slice()]);

    let account = &mut vm.rich_accounts[0];

    // Firstly, let's ensure that the contract works.
    let tx1 = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(precompiles_contract_address),
            calldata: call_code_oracle_function
                .encode_input(&[
                    Token::FixedBytes(big_zkevm_bytecode_hash.0.to_vec()),
                    Token::FixedBytes(big_zkevm_bytecode_keccak_hash.to_vec()),
                ])
                .unwrap(),
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );

    vm.vm.push_transaction(tx1);
    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful: {result:#?}"
    );
}

pub(crate) fn test_refunds_in_code_oracle<VM: TestedVm>() {
    let precompiles_contract_address = Address::repeat_byte(1);

    let normal_zkevm_bytecode = TestContract::counter().bytecode;
    let normal_zkevm_bytecode_hash = BytecodeHash::for_bytecode(normal_zkevm_bytecode).value();
    let normal_zkevm_bytecode_keccak_hash = keccak256(normal_zkevm_bytecode);
    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&normal_zkevm_bytecode_hash),
        u256_to_h256(U256::one()),
    );

    let precompile_contract = TestContract::precompiles_test();
    let call_code_oracle_function = precompile_contract.function("callCodeOracle");

    // Execute code oracle twice with identical VM state that only differs in that the queried bytecode
    // is already decommitted the second time. The second call must consume less gas (`decommit` doesn't charge additional gas
    // for already decommitted codes).
    let mut oracle_costs = vec![];
    for decommit in [false, true] {
        let mut vm = VmTesterBuilder::new()
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
            .with_custom_contracts(vec![ContractToDeploy::new(
                TestContract::precompiles_test().bytecode.to_vec(),
                precompiles_contract_address,
            )])
            .with_storage(storage.clone())
            .build::<VM>();

        vm.vm.insert_bytecodes(&[normal_zkevm_bytecode]);

        let account = &mut vm.rich_accounts[0];
        if decommit {
            let is_fresh = vm.vm.manually_decommit(normal_zkevm_bytecode_hash);
            assert!(is_fresh);
        }

        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(precompiles_contract_address),
                calldata: call_code_oracle_function
                    .encode_input(&[
                        Token::FixedBytes(normal_zkevm_bytecode_hash.0.to_vec()),
                        Token::FixedBytes(normal_zkevm_bytecode_keccak_hash.to_vec()),
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
        let log =
            find_code_oracle_cost_log(precompiles_contract_address, &result.logs.storage_logs);
        oracle_costs.push(log.log.value);
    }

    // The refund is equal to `gasCost` parameter passed to the `decommit` opcode, which is defined as `4 * contract_length_in_words`
    // in `CodeOracle.yul`.
    let code_oracle_refund = h256_to_u256(oracle_costs[0]) - h256_to_u256(oracle_costs[1]);
    assert_eq!(
        code_oracle_refund,
        (4 * (normal_zkevm_bytecode.len() / 32)).into()
    );
}
