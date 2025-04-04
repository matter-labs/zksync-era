use crate::{
    versions::testonly::transfer::{
        test_reentrancy_protection_send_and_transfer, test_send_and_transfer,
    },
    vm_1_5_0::{HistoryEnabled, Vm},
};

#[test]
fn send_and_transfer() {
    test_send_and_transfer::<Vm<_, HistoryEnabled>>();
}

#[test]
fn reentrancy_protection_send_and_transfer() {
    test_reentrancy_protection_send_and_transfer::<Vm<_, HistoryEnabled>>();
}
