use crate::{
    versions::testonly::simple_execution::{test_estimate_fee, test_simple_execute},
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn estimate_fee() {
    test_estimate_fee::<Vm<_, HistoryEnabled>>();
}

#[test]
fn simple_execute() {
    test_simple_execute::<Vm<_, HistoryEnabled>>();
}
