use crate::{
    versions::testonly::transfer::{
        test_reentrancy_protection_send_and_transfer, test_send_and_transfer,
    },
    vm_fast::Vm,
};

#[test]
fn send_and_transfer() {
    test_send_and_transfer::<Vm<_>>();
}

#[test]
fn reentrancy_protection_send_and_transfer() {
    test_reentrancy_protection_send_and_transfer::<Vm<_>>();
}
