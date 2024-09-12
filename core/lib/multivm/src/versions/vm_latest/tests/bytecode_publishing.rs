use crate::{
    interface::{TxExecutionMode, VmEvent, VmExecutionMode, VmInterface, VmInterfaceExt},
    utils::bytecode,
    vm_latest::{
        tests::{
            tester::{DeployContractsTx, TxType, VmTesterBuilder},
            utils::read_test_contract,
        },
        HistoryEnabled,
    },
};

#[test]
fn test_bytecode_publishing() {
    // In this test, we aim to ensure that the contents of the compressed bytecodes
    // are included as part of the L2->L1 long messages
    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let counter = read_test_contract();
    let account = &mut vm.rich_accounts[0];

    let compressed_bytecode = bytecode::compress(counter.clone()).unwrap().compressed;

    let DeployContractsTx { tx, .. } = account.get_deploy_tx(&counter, None, TxType::L2);
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    vm.vm.execute(VmExecutionMode::Batch);

    let state = vm.vm.get_current_execution_state();
    let long_messages = VmEvent::extract_long_l2_to_l1_messages(&state.events);
    assert!(
        long_messages.contains(&compressed_bytecode),
        "Bytecode not published"
    );
}
