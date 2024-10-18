use crate::{
    versions::testonly::storage::{test_storage_behavior, test_transient_storage_behavior},
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn storage_behavior() {
    test_storage_behavior::<Vm<_, HistoryEnabled>>();
}

#[test]
fn transient_storage_behavior() {
    test_transient_storage_behavior::<Vm<_, HistoryEnabled>>();
}
