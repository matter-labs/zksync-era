use zksync_types::Address;

use crate::{
    tracers::{TracerDispatcher, ValidationTracer},
    versions::testonly::{
        account_validation_rules::{
            test_account_validation_rules, test_validation_out_of_gas_with_fast_tracer,
            test_validation_out_of_gas_with_full_tracer,
        },
        inspect_oneshot_dump, load_vm_dump, mock_validation_params,
    },
    vm_latest::{HistoryEnabled, Vm},
    MultiVmTracer, VmVersion,
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
    let validation_tracer =
        ValidationTracer::<HistoryEnabled>::new(validation_params, VmVersion::latest(), 1);
    let validation_result = validation_tracer.get_result();
    let validation_tracer: Box<dyn MultiVmTracer<_, HistoryEnabled>> =
        validation_tracer.into_tracer_pointer();
    let tracers = TracerDispatcher::from(validation_tracer);

    let res = inspect_oneshot_dump::<Vm<_, HistoryEnabled>>(vm_dump, &mut tracers.into());
    let violated_rule = validation_result.get();
    assert!(violated_rule.is_none(), "{violated_rule:?}");
    assert!(!res.result.is_failed(), "{:?}", res.result);
}
