use crate::{
    versions::testonly::account_validation_rules::{
        test_account_validation_rules, test_validation_out_of_gas_with_fast_tracer,
        test_validation_out_of_gas_with_full_tracer,
    },
    vm_latest::Vm,
};

#[test]
fn account_validation_rules() {
    test_account_validation_rules::<Vm<_, _>>();
}

#[test]
fn validation_out_of_gas_with_full_tracer() {
    test_validation_out_of_gas_with_full_tracer::<Vm<_, _>>();
}

#[test]
fn validation_out_of_gas_with_fast_tracer() {
    test_validation_out_of_gas_with_fast_tracer::<Vm<_, _>>();
}
