use crate::{
    versions::testonly::evm::{
        test_calling_era_contract_from_evm, test_era_vm_deployment_after_evm_deployment,
        test_era_vm_deployment_after_evm_execution, test_real_emulator_basics,
        test_real_emulator_block_info, test_real_emulator_code_hash, test_real_emulator_deployment,
        test_real_emulator_msg_info, test_real_emulator_recursion,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn real_emulator_basics() {
    test_real_emulator_basics::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_code_hash() {
    test_real_emulator_code_hash::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_block_info() {
    test_real_emulator_block_info::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_msg_info() {
    test_real_emulator_msg_info::<Vm<_, HistoryEnabled>>();
}

#[ignore] // FIXME: stipend overflow for far call recursion
#[test]
fn real_emulator_recursion() {
    test_real_emulator_recursion::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_deployment() {
    test_real_emulator_deployment::<Vm<_, HistoryEnabled>>();
}

#[ignore] // FIXME: doesn't deploy the EraVM contract (doesn't store address -> code hash mapping)
#[test]
fn era_vm_deployment_after_evm_execution() {
    test_era_vm_deployment_after_evm_execution::<Vm<_, HistoryEnabled>>();
}

#[ignore] // FIXME: doesn't deploy the EraVM contract (doesn't store address -> code hash mapping)
#[test]
fn era_vm_deployment_after_evm_deployment() {
    test_era_vm_deployment_after_evm_deployment::<Vm<_, HistoryEnabled>>();
}

#[test]
fn calling_era_contract_from_evm() {
    test_calling_era_contract_from_evm::<Vm<_, HistoryEnabled>>();
}
