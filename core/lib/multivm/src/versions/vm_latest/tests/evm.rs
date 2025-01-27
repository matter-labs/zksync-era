use crate::{
    versions::testonly::evm::{
        test_calling_ecrecover_precompile, test_calling_era_contract_from_evm,
        test_calling_sha256_precompile, test_deployment_with_partial_reverts, test_emitted_events,
        test_era_vm_deployment_after_evm_deployment, test_era_vm_deployment_after_evm_execution,
        test_evm_bytecode_decommit, test_evm_deployment_tx, test_far_calls_from_evm_contract,
        test_real_emulator_basics, test_real_emulator_block_info, test_real_emulator_code_hash,
        test_real_emulator_deployment, test_real_emulator_gas_management,
        test_real_emulator_msg_info, test_real_emulator_recursion,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn evm_deployment_tx() {
    test_evm_deployment_tx::<Vm<_, HistoryEnabled>>();
}

#[test]
fn evm_bytecode_decommit() {
    test_evm_bytecode_decommit::<Vm<_, HistoryEnabled>>();
}

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

#[test]
fn real_emulator_gas_management() {
    test_real_emulator_gas_management::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_recursion() {
    test_real_emulator_recursion::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_deployment() {
    test_real_emulator_deployment::<Vm<_, HistoryEnabled>>();
}

#[test]
fn deployment_with_partial_reverts() {
    test_deployment_with_partial_reverts::<Vm<_, HistoryEnabled>>();
}

#[test]
fn era_vm_deployment_after_evm_execution() {
    test_era_vm_deployment_after_evm_execution::<Vm<_, HistoryEnabled>>();
}

#[test]
fn era_vm_deployment_after_evm_deployment() {
    test_era_vm_deployment_after_evm_deployment::<Vm<_, HistoryEnabled>>();
}

#[test]
fn calling_era_contract_from_evm() {
    test_calling_era_contract_from_evm::<Vm<_, HistoryEnabled>>();
}

#[test]
fn far_calls_from_evm_contract() {
    test_far_calls_from_evm_contract::<Vm<_, HistoryEnabled>>();
}

#[test]
fn emitting_events() {
    test_emitted_events::<Vm<_, HistoryEnabled>>();
}

#[test]
fn calling_sha256_precompile() {
    test_calling_sha256_precompile::<Vm<_, HistoryEnabled>>();
}

#[test]
fn calling_ecrecover_precompile() {
    test_calling_ecrecover_precompile::<Vm<_, HistoryEnabled>>();
}
