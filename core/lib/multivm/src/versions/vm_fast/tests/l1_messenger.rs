use crate::{versions::testonly::l1_messenger::test_rollup_da_output_hash_match, vm_fast::Vm};

#[test]
fn rollup_da_output_hash_match() {
    test_rollup_da_output_hash_match::<Vm<_>>();
}
