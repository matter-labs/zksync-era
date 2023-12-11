use zksync_types::U256;

use crate::interface::{ExecutionResult, Halt, TxExecutionMode, VmExecutionMode, VmInterface};
use crate::vm_virtual_blocks::constants::BOOTLOADER_HEAP_PAGE;
use crate::vm_virtual_blocks::tests::tester::VmTesterBuilder;
use crate::vm_virtual_blocks::tests::utils::{
    get_bootloader, verify_required_memory, BASE_SYSTEM_CONTRACTS,
};

use crate::vm_latest::HistoryEnabled;

#[test]
fn test_dummy_bootloader() {
    let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
    base_system_contracts.bootloader = get_bootloader("dummy");

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(base_system_contracts)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let result = vm.vm.execute(VmExecutionMode::Batch);
    assert!(!result.result.is_failed());

    let correct_first_cell = U256::from_str_radix("123123123", 16).unwrap();
    verify_required_memory(
        &vm.vm.state,
        vec![(correct_first_cell, BOOTLOADER_HEAP_PAGE, 0)],
    );
}

#[test]
fn test_bootloader_out_of_gas() {
    let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
    base_system_contracts.bootloader = get_bootloader("dummy");

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(base_system_contracts)
        .with_gas_limit(10)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let res = vm.vm.execute(VmExecutionMode::Batch);

    assert!(matches!(
        res.result,
        ExecutionResult::Halt {
            reason: Halt::BootloaderOutOfGas
        }
    ));
}
