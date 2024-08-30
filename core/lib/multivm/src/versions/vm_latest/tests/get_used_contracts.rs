use std::{
    collections::{HashMap, HashSet},
    iter,
    str::FromStr,
};

use assert_matches::assert_matches;
use ethabi::Token;
use itertools::Itertools;
use zk_evm_1_3_1::zkevm_opcode_defs::decoding::{EncodingModeProduction, VmEncodingMode};
use zk_evm_1_5_0::{
    abstractions::DecommittmentProcessor,
    aux_structures::{DecommittmentQuery, MemoryPage, Timestamp},
    zkevm_opcode_defs::{VersionedHashHeader, VersionedHashNormalizedPreimage},
};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{Address, Execute, U256};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};
use zksync_vm_interface::VmExecutionResultAndLogs;

use crate::{
    interface::{
        storage::WriteStorage, ExecutionResult, TxExecutionMode, VmExecutionMode, VmInterface,
        VmInterfaceExt,
    },
    vm_latest::{
        tests::{
            tester::{TxType, VmTester, VmTesterBuilder},
            utils::{read_proxy_counter_contract, read_test_contract, BASE_SYSTEM_CONTRACTS},
        },
        HistoryDisabled, Vm,
    },
    HistoryMode,
};

#[test]
fn test_get_used_contracts() {
    let mut vm = VmTesterBuilder::new(HistoryDisabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    assert!(known_bytecodes_without_base_system_contracts(&vm.vm).is_empty());

    // create and push and execute some not-empty factory deps transaction with success status
    // to check that `get_used_contracts()` updates
    let contract_code = read_test_contract();
    let mut account = Account::random();
    let tx = account.get_deploy_tx(&contract_code, None, TxType::L1 { serial_id: 0 });
    vm.vm.push_transaction(tx.tx.clone());
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed());

    assert!(vm
        .vm
        .get_used_contracts()
        .contains(&h256_to_u256(tx.bytecode_hash)));

    // Note: `Default_AA` will be in the list of used contracts if L2 tx is used
    assert_eq!(
        vm.vm
            .get_used_contracts()
            .into_iter()
            .collect::<HashSet<U256>>(),
        known_bytecodes_without_base_system_contracts(&vm.vm)
            .keys()
            .cloned()
            .collect::<HashSet<U256>>()
    );

    // create push and execute some non-empty factory deps transaction that fails
    // (`known_bytecodes` will be updated but we expect `get_used_contracts()` to not be updated)

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
            contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
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
        assert!(known_bytecodes_without_base_system_contracts(&vm.vm)
            .keys()
            .contains(&hash_to_u256));
        assert!(!vm.vm.get_used_contracts().contains(&hash_to_u256));
    }
}

#[test]
fn test_contract_is_used_right_after_prepare_to_decommit() {
    let mut vm = VmTesterBuilder::new(HistoryDisabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    assert!(vm.vm.get_used_contracts().is_empty());

    let bytecode_hash =
        U256::from_str("0x100067ff3124f394104ab03481f7923f0bc4029a2aa9d41cc1d848c81257185")
            .unwrap();
    vm.vm
        .state
        .decommittment_processor
        .populate(vec![(bytecode_hash, vec![])], Timestamp(0));

    let header = hex::decode("0100067f").unwrap();
    let normalized_preimage =
        hex::decode("f3124f394104ab03481f7923f0bc4029a2aa9d41cc1d848c81257185").unwrap();
    vm.vm
        .state
        .decommittment_processor
        .prepare_to_decommit(
            0,
            DecommittmentQuery {
                header: VersionedHashHeader(header.try_into().unwrap()),
                normalized_preimage: VersionedHashNormalizedPreimage(
                    normalized_preimage.try_into().unwrap(),
                ),
                timestamp: Timestamp(0),
                memory_page: MemoryPage(0),
                decommitted_length: 0,
                is_fresh: false,
            },
        )
        .unwrap();

    assert_eq!(vm.vm.get_used_contracts(), vec![bytecode_hash]);
}

fn known_bytecodes_without_base_system_contracts<S: WriteStorage, H: HistoryMode>(
    vm: &Vm<S, H>,
) -> HashMap<U256, Vec<U256>> {
    let mut known_bytecodes_without_base_system_contracts = vm
        .state
        .decommittment_processor
        .known_bytecodes
        .inner()
        .clone();
    known_bytecodes_without_base_system_contracts
        .remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash))
        .unwrap();

    known_bytecodes_without_base_system_contracts
        .remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.evm_simulator.hash))
        .unwrap();
    known_bytecodes_without_base_system_contracts
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

fn execute_proxy_counter(gas: u32) -> (VmTester<HistoryDisabled>, U256, VmExecutionResultAndLogs) {
    let counter_bytecode = inflated_counter_bytecode();
    let counter_bytecode_hash = h256_to_u256(hash_bytecode(&counter_bytecode));
    let counter_address = Address::repeat_byte(0x23);

    let mut vm = VmTesterBuilder::new(HistoryDisabled)
        .with_empty_in_memory_storage()
        .with_custom_contracts(vec![(counter_bytecode, counter_address, false)])
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

    let decommitted_hashes = vm.vm.get_used_contracts();
    assert!(
        !decommitted_hashes.contains(&counter_bytecode_hash),
        "{decommitted_hashes:?}"
    );

    let increment = proxy_counter_abi.function("increment").unwrap();
    let increment_tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(deploy_tx.address),
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
    (vm, counter_bytecode_hash, exec_result)
}

#[test]
fn get_used_contracts_with_far_call() {
    let (vm, counter_bytecode_hash, exec_result) = execute_proxy_counter(100_000);
    assert!(!exec_result.result.is_failed(), "{exec_result:#?}");
    let decommitted_hashes = vm.vm.get_used_contracts();
    assert!(
        decommitted_hashes.contains(&counter_bytecode_hash),
        "{decommitted_hashes:?}"
    );
}

#[test]
fn get_used_contracts_with_out_of_gas_far_call() {
    let (vm, counter_bytecode_hash, exec_result) = execute_proxy_counter(10_000);
    assert_matches!(exec_result.result, ExecutionResult::Revert { .. });
    let decommitted_hashes = vm.vm.get_used_contracts();
    assert!(
        decommitted_hashes.contains(&counter_bytecode_hash),
        "{decommitted_hashes:?}"
    );
}
