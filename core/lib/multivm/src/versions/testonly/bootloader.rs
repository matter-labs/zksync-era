use assert_matches::assert_matches;
use zksync_types::U256;

use super::{get_bootloader, tester::VmTesterBuilder, TestedVm, BASE_SYSTEM_CONTRACTS};
use crate::interface::{ExecutionResult, Halt, TxExecutionMode};

pub(crate) fn test_dummy_bootloader<VM: TestedVm>() {
    let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
    base_system_contracts.bootloader = get_bootloader("dummy");

    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(base_system_contracts)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let result = vm.vm.finish_batch_without_pubdata();
    assert!(!result.result.is_failed());

    let correct_first_cell = U256::from_str_radix("123123123", 16).unwrap();
    vm.vm
        .verify_required_bootloader_heap(&[(0, correct_first_cell)]);
}

pub(crate) fn test_bootloader_out_of_gas<VM: TestedVm>() {
    let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
    base_system_contracts.bootloader = get_bootloader("dummy");

    let mut vm = VmTesterBuilder::new()
        .with_base_system_smart_contracts(base_system_contracts)
        .with_bootloader_gas_limit(10)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<VM>();

    let res = vm.vm.finish_batch_without_pubdata();

    assert_matches!(
        res.result,
        ExecutionResult::Halt {
            reason: Halt::BootloaderOutOfGas
        }
    );
}
