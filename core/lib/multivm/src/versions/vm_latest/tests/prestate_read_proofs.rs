use crate::{
    versions::testonly::prestate_read_proofs::{
        test_known_code_marker_read_outside_vm, test_rolled_back_write_prestate_is_observable,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn rolled_back_write_prestate_is_observable() {
    test_rolled_back_write_prestate_is_observable::<Vm<_, HistoryEnabled>, Vm<_, HistoryEnabled>>();
}

#[test]
fn known_code_marker_read_outside_vm() {
    test_known_code_marker_read_outside_vm::<Vm<_, HistoryEnabled>, Vm<_, HistoryEnabled>>();
}
