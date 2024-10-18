use assert_matches::assert_matches;
use zksync_types::U256;
use zksync_vm2::interface::HeapId;
use zksync_vm_interface::VmInterface;

use crate::{
    interface::{ExecutionResult, Halt, TxExecutionMode},
    versions::{
        testonly::default_pubdata_builder,
        vm_fast::tests::{
            tester::VmTesterBuilder,
            utils::{get_bootloader, verify_required_memory, BASE_SYSTEM_CONTRACTS},
        },
    },
};

#[test]
fn test_dummy_bootloader() {
    let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
    base_system_contracts.bootloader = get_bootloader("dummy");

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(base_system_contracts)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let result = vm.vm.finish_batch(Some(default_pubdata_builder()));
    assert!(!result.block_tip_execution_result.result.is_failed());

    let correct_first_cell = U256::from_str_radix("123123123", 16).unwrap();

    verify_required_memory(&vm.vm.inner, vec![(correct_first_cell, HeapId::FIRST, 0)]);
}

#[test]
fn test_bootloader_out_of_gas() {
    let mut base_system_contracts = BASE_SYSTEM_CONTRACTS.clone();
    base_system_contracts.bootloader = get_bootloader("dummy");

    let mut vm = VmTesterBuilder::new()
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(base_system_contracts)
        .with_bootloader_gas_limit(10)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build();

    let res = vm.vm.finish_batch(Some(default_pubdata_builder()));

    assert_matches!(
        res.block_tip_execution_result.result,
        ExecutionResult::Halt {
            reason: Halt::BootloaderOutOfGas
        }
    );
}
