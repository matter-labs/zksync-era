use std::{collections::HashSet, iter};

use assert_matches::assert_matches;
use ethabi::Token;
use itertools::Itertools;
use zk_evm_1_3_1::zkevm_opcode_defs::decoding::{EncodingModeProduction, VmEncodingMode};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{AccountTreeId, Address, Execute, StorageKey, H256, U256};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::{
    interface::{
        storage::ReadStorage, ExecutionResult, TxExecutionMode, VmExecutionMode,
        VmExecutionResultAndLogs, VmInterface, VmInterfaceExt,
    },
    versions::testonly::ContractToDeploy,
    vm_fast::{
        tests::{
            tester::{TxType, VmTester, VmTesterBuilder},
            utils::{read_proxy_counter_contract, read_test_contract, BASE_SYSTEM_CONTRACTS},
        },
        vm::Vm,
    },
};

#[test]
fn test_get_used_contracts() {
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    assert!(known_bytecodes_without_aa_code(&vm.vm).is_empty());

    // create and push and execute some not-empty factory deps transaction with success status
    // to check that `get_decommitted_hashes()` updates
    let contract_code = read_test_contract();
    let mut account = Account::random();
    let tx = account.get_deploy_tx(&contract_code, None, TxType::L1 { serial_id: 0 });
    vm.vm.push_transaction(tx.tx.clone());
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    assert!(vm
        .vm
        .decommitted_hashes()
        .contains(&h256_to_u256(tx.bytecode_hash)));

    // Note: `Default_AA` will be in the list of used contracts if L2 tx is used
    assert_eq!(
        vm.vm.decommitted_hashes().collect::<HashSet<U256>>(),
        known_bytecodes_without_aa_code(&vm.vm)
    );

    // create push and execute some non-empty factory deps transaction that fails
    // (`known_bytecodes` will be updated but we expect `get_decommitted_hashes()` to not be updated)

    let calldata = [1, 2, 3];
    let big_calldata: Vec<u8> = calldata
        .iter()
        .cycle()
        .take(calldata.len() * 1024)
        .cloned()
        .collect();
    let account2 = Account::random();
    let tx2 = account2.get_l1_tx(
        Execute {
            contract_address: CONTRACT_DEPLOYER_ADDRESS,
            calldata: big_calldata,
            value: Default::default(),
            factory_deps: vec![vec![1; 32]],
        },
        1,
    );

    vm.vm.push_transaction(tx2.clone());

    let res2 = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(res2.result.is_failed());

    for factory_dep in tx2.execute.factory_deps {
        let hash = hash_bytecode(&factory_dep);
        let hash_to_u256 = h256_to_u256(hash);
        assert!(known_bytecodes_without_aa_code(&vm.vm).contains(&hash_to_u256));
        assert!(!vm.vm.decommitted_hashes().contains(&hash_to_u256));
    }
}

fn known_bytecodes_without_aa_code<S: ReadStorage>(vm: &Vm<S>) -> HashSet<U256> {
    let mut known_bytecodes_without_aa_code = vm
        .world
        .bytecode_cache
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    known_bytecodes_without_aa_code.remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash));
    known_bytecodes_without_aa_code
}

/// Counter test contract bytecode inflated by appending lots of `NOP` opcodes at the end. This leads to non-trivial
/// decommitment cost (>10,000 gas).
fn inflated_counter_bytecode() -> Vec<u8> {
    let mut counter_bytecode = read_test_contract();
    counter_bytecode.extend(
        iter::repeat(EncodingModeProduction::nop_encoding().to_be_bytes())
            .take(10_000)
            .flatten(),
    );
    counter_bytecode
}

#[derive(Debug)]
struct ProxyCounterData {
    proxy_counter_address: Address,
    counter_bytecode_hash: U256,
}

fn execute_proxy_counter(gas: u32) -> (VmTester, ProxyCounterData, VmExecutionResultAndLogs) {
    let counter_bytecode = inflated_counter_bytecode();
    let counter_bytecode_hash = h256_to_u256(hash_bytecode(&counter_bytecode));
    let counter_address = Address::repeat_byte(0x23);

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_custom_contracts(vec![ContractToDeploy::new(
            counter_bytecode,
            counter_address,
        )])
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let (proxy_counter_bytecode, proxy_counter_abi) = read_proxy_counter_contract();
    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(
        &proxy_counter_bytecode,
        Some(&[Token::Address(counter_address)]),
        TxType::L2,
    );
    let (compression_result, exec_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(deploy_tx.tx, true);
    compression_result.unwrap();
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let decommitted_hashes = vm.vm.decommitted_hashes().collect::<HashSet<_>>();
    assert!(
        !decommitted_hashes.contains(&counter_bytecode_hash),
        "{decommitted_hashes:?}"
    );

    let increment = proxy_counter_abi.function("increment").unwrap();
    let increment_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: deploy_tx.address,
            calldata: increment
                .encode_input(&[Token::Uint(1.into()), Token::Uint(gas.into())])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (compression_result, exec_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(increment_tx, true);
    compression_result.unwrap();
    let data = ProxyCounterData {
        proxy_counter_address: deploy_tx.address,
        counter_bytecode_hash,
    };
    (vm, data, exec_result)
}

#[test]
fn get_used_contracts_with_far_call() {
    let (vm, data, exec_result) = execute_proxy_counter(100_000);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");
    let decommitted_hashes = vm.vm.decommitted_hashes().collect::<HashSet<_>>();
    assert!(
        decommitted_hashes.contains(&data.counter_bytecode_hash),
        "{decommitted_hashes:?}"
    );
}

#[test]
fn get_used_contracts_with_out_of_gas_far_call() {
    let (mut vm, data, exec_result) = execute_proxy_counter(10_000);
    assert_matches!(exec_result.result, ExecutionResult::Revert { .. });
    let decommitted_hashes = vm.vm.decommitted_hashes().collect::<HashSet<_>>();
    assert!(
        decommitted_hashes.contains(&data.counter_bytecode_hash),
        "{decommitted_hashes:?}"
    );

    // Execute another transaction with a successful far call and check that it's still charged for decommitment.
    let account = &mut vm.rich_accounts[0];
    let (_, proxy_counter_abi) = read_proxy_counter_contract();
    let increment = proxy_counter_abi.function("increment").unwrap();
    let increment_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: data.proxy_counter_address,
            calldata: increment
                .encode_input(&[Token::Uint(1.into()), Token::Uint(u64::MAX.into())])
                .unwrap(),
            value: 0.into(),
            factory_deps: vec![],
        },
        None,
    );
    let (compression_result, exec_result) = vm
        .vm
        .execute_transaction_with_bytecode_compression(increment_tx, true);
    compression_result.unwrap();
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

    let proxy_counter_cost_key = StorageKey::new(
        AccountTreeId::new(data.proxy_counter_address),
        H256::from_low_u64_be(1),
    );
    let far_call_cost_log = exec_result
        .logs
        .storage_logs
        .iter()
        .find(|log| log.log.key == proxy_counter_cost_key)
        .expect("no cost log");
    assert!(
        far_call_cost_log.previous_value.is_zero(),
        "{far_call_cost_log:?}"
    );
    let far_call_cost = h256_to_u256(far_call_cost_log.log.value);
    assert!(far_call_cost > 10_000.into(), "{far_call_cost}");
}
