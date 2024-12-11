use crate::{
    versions::testonly::rollbacks::{
        test_rollback_in_call_mode, test_vm_loadnext_rollbacks, test_vm_rollbacks,
    },
    vm_fast::Vm,
};

#[test]
fn vm_rollbacks() {
    test_vm_rollbacks::<Vm<_>>();
}

#[test]
fn vm_loadnext_rollbacks() {
    test_vm_loadnext_rollbacks::<Vm<_>>();
}

#[test]
fn rollback_in_call_mode() {
    test_rollback_in_call_mode::<Vm<_>>();
}
