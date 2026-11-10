use super::TestedFastVm;
use crate::{versions::testonly::cycle_estimator, vm_fast::FastValidationTracer};

type Vm = TestedFastVm<(), FastValidationTracer>;

#[test]
fn basic_behavior() {
    cycle_estimator::test_basic_behavior::<Vm>();
}

#[test]
fn estimate_scales_with_batch_size() {
    cycle_estimator::test_estimate_scales_with_batch_size::<Vm>();
}

#[test]
fn repeat_decommit_is_counted() {
    cycle_estimator::test_repeat_decommit_is_counted::<Vm>();
}
