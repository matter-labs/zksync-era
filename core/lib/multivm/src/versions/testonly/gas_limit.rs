use zksync_test_account::Account;
use zksync_types::{fee::Fee, Execute};

use super::{tester::VmTesterBuilder, TestedVm};
use crate::{
    interface::TxExecutionMode,
    vm_latest::constants::{TX_DESCRIPTION_OFFSET, TX_GAS_LIMIT_OFFSET},
};

/// Checks that `TX_GAS_LIMIT_OFFSET` constant is correct.
pub(crate) fn test_tx_gas_limit_offset<VM: TestedVm>() {
    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .build::<VM>();

    let gas_limit = 9999.into();
    let tx = vm.rich_accounts[0].get_l2_tx_for_execute(
        Execute {
            contract_address: Some(Default::default()),
            ..Default::default()
        },
        Some(Fee {
            gas_limit,
            ..Account::default_fee()
        }),
    );

    vm.vm.push_transaction(tx);

    let slot = (TX_DESCRIPTION_OFFSET + TX_GAS_LIMIT_OFFSET) as u32;
    vm.vm.verify_required_bootloader_heap(&[(slot, gas_limit)]);
}
