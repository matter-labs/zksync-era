use zksync_state::ReadStorage;
use zksync_types::get_nonce_key;

use crate::{
    era_vm::tests::{
        tester::{Account, TxType, VmTesterBuilder},
        utils::read_test_contract,
    },
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
};

#[test]
fn test_is_write_initial_behaviour() {
    // In this test, we check result of `is_write_initial` at different stages.
    // The main idea is to check that `is_write_initial` storage uses the correct cache for initial_writes and doesn't
    // messed up it with the repeated writes during the one batch execution.

    let mut account = Account::random();
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(vec![account.clone()])
        .build();

    let nonce_key = get_nonce_key(&account.address);
    // Check that the next write to the nonce key will be initial.
    assert!(vm
        .storage
        .as_ref()
        .borrow_mut()
        .is_write_initial(&nonce_key));

    let contract_code = read_test_contract();
    let tx = account.get_deploy_tx(&contract_code, None, TxType::L2).tx;

    vm.vm.push_transaction(tx);
    vm.vm.execute(VmExecutionMode::OneTx);

    // Check that `is_write_initial` still returns true for the nonce key.
    assert!(vm
        .storage
        .as_ref()
        .borrow_mut()
        .is_write_initial(&nonce_key));
}
