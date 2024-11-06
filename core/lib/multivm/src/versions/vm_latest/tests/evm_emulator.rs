use test_casing::{test_casing, Product};

use crate::{
    versions::testonly::evm_emulator::{
        test_calling_to_mock_emulator_from_native_contract, test_mock_emulator_basics,
        test_mock_emulator_with_delegate_call, test_mock_emulator_with_deployment,
        test_mock_emulator_with_payment, test_mock_emulator_with_recursion,
        test_mock_emulator_with_static_call, test_tracing_evm_contract_deployment,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn tracing_evm_contract_deployment() {
    test_tracing_evm_contract_deployment::<Vm<_, HistoryEnabled>>();
}

#[test]
fn mock_emulator_basics() {
    test_mock_emulator_basics::<Vm<_, HistoryEnabled>>();
}

#[test_casing(2, [false, true])]
#[test]
fn mock_emulator_with_payment(deploy_emulator: bool) {
    test_mock_emulator_with_payment::<Vm<_, HistoryEnabled>>(deploy_emulator);
}

#[test_casing(4, Product(([false, true], [false, true])))]
#[test]
fn mock_emulator_with_recursion(deploy_emulator: bool, is_external: bool) {
    test_mock_emulator_with_recursion::<Vm<_, HistoryEnabled>>(deploy_emulator, is_external);
}

#[test]
fn calling_to_mock_emulator_from_native_contract() {
    test_calling_to_mock_emulator_from_native_contract::<Vm<_, HistoryEnabled>>();
}

#[test]
fn mock_emulator_with_deployment() {
    test_mock_emulator_with_deployment::<Vm<_, HistoryEnabled>>();
}

#[test]
fn mock_emulator_with_delegate_call() {
    test_mock_emulator_with_delegate_call::<Vm<_, HistoryEnabled>>();
}

#[test]
fn mock_emulator_with_static_call() {
    test_mock_emulator_with_static_call::<Vm<_, HistoryEnabled>>();
}
