use super::TestedLatestVm;
use crate::versions::testonly::cycle_estimator;

#[test]
fn basic_behavior() {
    cycle_estimator::test_basic_behavior::<TestedLatestVm>();
}

#[test]
fn estimate_scales_with_batch_size() {
    cycle_estimator::test_estimate_scales_with_batch_size::<TestedLatestVm>();
}

#[test]
fn repeat_decommit_is_counted() {
    cycle_estimator::test_repeat_decommit_is_counted::<TestedLatestVm>();
}
