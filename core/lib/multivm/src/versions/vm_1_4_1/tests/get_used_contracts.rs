use std::collections::{HashMap, HashSet};

use itertools::Itertools;
use crate::interface::storage::WriteStorage;
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{Execute, U256};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_1_4_1::{
        tests::{
            tester::{TxType, VmTesterBuilder},
            utils::{read_test_contract, BASE_SYSTEM_CONTRACTS},
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
            contract_address: CONTRACT_DEPLOYER_ADDRESS,
            calldata: big_calldata,
            value: Default::default(),
            factory_deps: Some(vec![vec![1; 32]]),
        },
        1,
    );

    vm.vm.push_transaction(tx2.clone());

    let res2 = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(res2.result.is_failed());

    for factory_dep in tx2.execute.factory_deps.unwrap() {
        let hash = hash_bytecode(&factory_dep);
        let hash_to_u256 = h256_to_u256(hash);
        assert!(known_bytecodes_without_base_system_contracts(&vm.vm)
            .keys()
            .contains(&hash_to_u256));
        assert!(!vm.vm.get_used_contracts().contains(&hash_to_u256));
    }
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
