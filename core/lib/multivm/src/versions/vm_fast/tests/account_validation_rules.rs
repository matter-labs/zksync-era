use zksync_types::Address;

use super::TestedFastVm;
use crate::{
    versions::testonly::{
        account_validation_rules::{
            test_account_validation_rules, test_validation_out_of_gas_with_fast_tracer,
            test_validation_out_of_gas_with_full_tracer,
        },
        inspect_oneshot_dump, load_vm_dump, mock_validation_params,
    },
    vm_fast::{self, FastValidationTracer, FullValidationTracer},
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

#[test]
fn adjacent_storage_slots_are_allowed() {
    let vm_dump = load_vm_dump("validation_adjacent_storage_slots");
    let tx = &vm_dump.l2_blocks[0].txs[0];
    let accessed_tokens = [
        "0x185db4e63ff8afdc67b2e44acaa837c104eb22bc",
        "0x272814b0125380dc65a63570abf903d0a434b597",
    ]
    .map(|s| s.parse::<Address>().unwrap());
    let validation_params = mock_validation_params(tx, &accessed_tokens);
    let mut tracers = ((), FullValidationTracer::new(validation_params, 1));

    let res = inspect_oneshot_dump::<vm_fast::Vm<_, _, _>>(vm_dump, &mut tracers);
    let violated_rule = tracers.1.validation_error();
    assert!(violated_rule.is_none(), "{violated_rule:?}");
    assert!(!res.result.is_failed(), "{:?}", res.result);
}
