use zksync_test_account::TxType;
use zksync_types::{get_code_key, H256, SYSTEM_CONTEXT_ADDRESS};

use super::{get_empty_storage, read_test_contract, tester::VmTesterBuilder, TestedVm};
use crate::interface::{TxExecutionMode, VmExecutionMode, VmInterfaceExt};

/// This test checks that the new bootloader will work fine even if the previous system context contract is not
/// compatible with it, i.e. the bootloader will upgrade it before starting any transaction.
pub(crate) fn test_migration_for_system_context_aa_interaction<VM: TestedVm>() {
    let mut storage = get_empty_storage();
    // We will set the system context bytecode to zero.
    storage.set_value(get_code_key(&SYSTEM_CONTEXT_ADDRESS), H256::zero());

    // In this test, we aim to test whether a simple account interaction (without any fee logic)
    // will work. The account will try to deploy a simple contract from integration tests.
    let mut vm = VmTesterBuilder::new()
        .with_storage(storage)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build::<VM>();

    // Now, we will just proceed with standard transaction execution.
    // The bootloader should be able to update system context regardless of whether
    // the upgrade transaction is there or not.
    let account = &mut vm.rich_accounts[0];
    let counter = read_test_contract();
    let tx = account.get_deploy_tx(&counter, None, TxType::L2).tx;

    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(
        !result.result.is_failed(),
        "Transaction wasn't successful {:#?}",
        result.result
    );

    let batch_result = vm.vm.execute(VmExecutionMode::Batch);
    assert!(
        !batch_result.result.is_failed(),
        "Batch transaction wasn't successful {:#?}",
        batch_result.result
    );
}
