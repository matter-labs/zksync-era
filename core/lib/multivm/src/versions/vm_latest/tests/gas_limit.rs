use zksync_test_account::Account;
use zksync_types::{fee::Fee, Execute};

use crate::{
    interface::{TxExecutionMode, VmInterface},
    vm_latest::{
        constants::{BOOTLOADER_HEAP_PAGE, TX_DESCRIPTION_OFFSET, TX_GAS_LIMIT_OFFSET},
        tests::tester::VmTesterBuilder,
        HistoryDisabled,
    },
};

/// Checks that `TX_GAS_LIMIT_OFFSET` constant is correct.
#[test]
fn test_tx_gas_limit_offset() {
    let mut vm = VmTesterBuilder::new(HistoryDisabled)
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

    let gas_limit_from_memory = vm
        .vm
        .state
        .memory
        .read_slot(
            BOOTLOADER_HEAP_PAGE as usize,
            TX_DESCRIPTION_OFFSET + TX_GAS_LIMIT_OFFSET,
        )
        .value;
    assert_eq!(gas_limit_from_memory, gas_limit);
}
