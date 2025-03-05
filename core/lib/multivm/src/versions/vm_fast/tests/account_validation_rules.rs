use super::TestedFastVm;
use crate::{
    versions::testonly::account_validation_rules::{
        test_account_validation_rules, test_validation_out_of_gas_with_fast_tracer,
        test_validation_out_of_gas_with_full_tracer,
    },
    vm_fast::FastValidationTracer,
};

#[test]
fn account_validation_rules() {
    test_account_validation_rules::<TestedFastVm<(), _>>();
}

#[test]
fn validation_out_of_gas_with_full_tracer() {
    test_validation_out_of_gas_with_full_tracer::<TestedFastVm<(), _>>();
}

#[test]
fn validation_out_of_gas_with_fast_tracer() {
    test_validation_out_of_gas_with_fast_tracer::<TestedFastVm<(), FastValidationTracer>>();
}
