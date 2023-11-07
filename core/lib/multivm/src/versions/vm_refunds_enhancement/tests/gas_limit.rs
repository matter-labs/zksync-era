use zksync_types::fee::Fee;
use zksync_types::Execute;

use crate::vm_refunds_enhancement::constants::{
    BOOTLOADER_HEAP_PAGE, TX_DESCRIPTION_OFFSET, TX_GAS_LIMIT_OFFSET,
};
use crate::vm_refunds_enhancement::tests::tester::VmTesterBuilder;

use crate::interface::TxExecutionMode;
use crate::vm_refunds_enhancement::HistoryDisabled;

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
        Execute {
            contract_address: Default::default(),
            calldata: vec![],
            value: Default::default(),
            factory_deps: None,
        },
        Some(Fee {
            gas_limit,
            ..Default::default()
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
