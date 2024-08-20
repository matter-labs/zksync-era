use std::collections::HashSet;

use itertools::Itertools;
use zksync_state::ReadStorage;
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_test_account::Account;
use zksync_types::{Execute, U256};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::{
    era_vm::{
        tests::{
            tester::{TxType, VmTesterBuilder},
            utils::{read_test_contract, BASE_SYSTEM_CONTRACTS},
        },
        vm::Vm,
    },
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
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
        .inner
        .state
        .decommitted_hashes()
        .contains(&h256_to_u256(tx.bytecode_hash)));

    // Note: `Default_AA` will be in the list of used contracts if L2 tx is used
    assert_eq!(
        *vm.vm.inner.state.decommitted_hashes(),
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
        assert!(!vm
            .vm
            .inner
            .state
            .decommitted_hashes()
            .contains(&hash_to_u256));
    }
}

fn known_bytecodes_without_aa_code<S: ReadStorage>(vm: &Vm<S>) -> HashSet<U256> {
    let mut known_bytecodes_without_aa_code = vm
        .program_cache
        .borrow()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();

    known_bytecodes_without_aa_code.remove(&h256_to_u256(BASE_SYSTEM_CONTRACTS.default_aa.hash));

    known_bytecodes_without_aa_code
}
