use crate::{versions::testonly::block_tip::test_dry_run_upper_bound, vm_fast::Vm};

#[test]
fn dry_run_upper_bound() {
    test_dry_run_upper_bound::<Vm<_>>();
}
