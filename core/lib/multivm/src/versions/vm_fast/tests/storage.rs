use crate::{
    versions::testonly::storage::{test_storage_behavior, test_transient_storage_behavior},
    vm_fast::Vm,
};

#[test]
fn storage_behavior() {
    test_storage_behavior::<Vm<_>>();
}

#[test]
fn transient_storage_behavior() {
    test_transient_storage_behavior::<Vm<_>>();
}
