use zksync_test_contracts::{TestContract, TxType};
use zksync_types::get_nonce_key;

use super::{tester::VmTesterBuilder, TestedVm};
use crate::interface::{
    storage::ReadStorage, InspectExecutionMode, TxExecutionMode, VmInterfaceExt,
};

pub(crate) fn test_is_write_initial_behaviour<VM: TestedVm>() {
    // In this test, we check result of `is_write_initial` at different stages.
    // The main idea is to check that `is_write_initial` storage uses the correct cache for initial_writes and doesn't
    // messed up it with the repeated writes during the one batch execution.
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();
    let account = &mut vm.rich_accounts[0];

    let nonce_key = get_nonce_key(&account.address);
    // Check that the next write to the nonce key will be initial.
    assert!(vm
        .storage
        .as_ref()
        .borrow_mut()
        .is_write_initial(&nonce_key));

    let tx = account
        .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
        .tx;

    vm.vm.push_transaction(tx);
    vm.vm.execute(InspectExecutionMode::OneTx);

    // Check that `is_write_initial` still returns true for the nonce key.
    assert!(vm
        .storage
        .as_ref()
        .borrow_mut()
        .is_write_initial(&nonce_key));
}
