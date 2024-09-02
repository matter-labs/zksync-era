use zksync_test_account::Account;
use zksync_types::{fee::Fee, Execute};

use crate::{
    interface::{TxExecutionMode, VmInterface},
    vm_fast::tests::tester::VmTesterBuilder,
    vm_latest::constants::{TX_DESCRIPTION_OFFSET, TX_GAS_LIMIT_OFFSET},
};

/// Checks that `TX_GAS_LIMIT_OFFSET` constant is correct.
#[test]
fn test_tx_gas_limit_offset() {
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let gas_limit = 9999.into();
    let tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute::default(),
        Some(Fee {
            gas_limit,
            ..Account::default_fee()
        }),
    );

    vm.vm.push_transaction(tx);

    assert!(vm.vm.inner.state.previous_frames.is_empty());
    let gas_limit_from_memory = vm
        .vm
        .read_word_from_bootloader_heap(TX_DESCRIPTION_OFFSET + TX_GAS_LIMIT_OFFSET);

    assert_eq!(gas_limit_from_memory, gas_limit);
}
