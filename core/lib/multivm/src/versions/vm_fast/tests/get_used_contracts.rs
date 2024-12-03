use crate::{
    versions::testonly::get_used_contracts::{
        test_get_used_contracts, test_get_used_contracts_with_far_call,
        test_get_used_contracts_with_out_of_gas_far_call,
    },
    vm_fast::Vm,
};

#[test]
fn get_used_contracts() {
    test_get_used_contracts::<Vm<_>>();
}

#[test]
fn get_used_contracts_with_far_call() {
    test_get_used_contracts_with_far_call::<Vm<_>>();
}

#[test]
fn get_used_contracts_with_out_of_gas_far_call() {
    test_get_used_contracts_with_out_of_gas_far_call::<Vm<_>>();
}
