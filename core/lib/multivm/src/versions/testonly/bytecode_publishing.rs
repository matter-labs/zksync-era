use zksync_test_contracts::{TestContract, TxType};

use super::{default_pubdata_builder, tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmEvent, VmInterfaceExt},
    utils::bytecode,
};

pub(crate) fn test_bytecode_publishing<VM: TestedVm>() {
    // In this test, we aim to ensure that the contents of the compressed bytecodes
    // are included as part of the L2->L1 long messages
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let counter = TestContract::counter().bytecode;
    let account = &mut vm.rich_accounts[0];

    let compressed_bytecode = bytecode::compress(counter.to_vec()).unwrap().compressed;

    let tx = account.get_deploy_tx(counter, None, TxType::L2).tx;
    assert_eq!(tx.execute.factory_deps.len(), 1); // The deployed bytecode is the only dependency
    let push_result = vm.vm.push_transaction(tx);
    assert_eq!(push_result.compressed_bytecodes.len(), 1);
    assert_eq!(push_result.compressed_bytecodes[0].original, counter);
    assert_eq!(
        push_result.compressed_bytecodes[0].compressed,
        compressed_bytecode
    );

    let result = vm.vm.execute(InspectExecutionMode::OneTx);
    assert!(!result.result.is_failed(), "Transaction wasn't successful");

    vm.vm.finish_batch(default_pubdata_builder());

    let state = vm.vm.get_current_execution_state();
    let long_messages = VmEvent::extract_long_l2_to_l1_messages(&state.events);
    assert!(
        long_messages.contains(&compressed_bytecode),
        "Bytecode not published"
    );
}
