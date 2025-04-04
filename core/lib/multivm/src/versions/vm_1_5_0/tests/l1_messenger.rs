use crate::{
    versions::testonly::l1_messenger::test_rollup_da_output_hash_match,
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn rollup_da_output_hash_match() {
    test_rollup_da_output_hash_match::<Vm<_, HistoryEnabled>>();
}
