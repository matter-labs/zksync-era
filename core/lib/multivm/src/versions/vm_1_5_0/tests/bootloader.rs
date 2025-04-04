use crate::{
    versions::testonly::bootloader::{test_bootloader_out_of_gas, test_dummy_bootloader},
    vm_1_5_0::Vm,
    vm_latest::HistoryEnabled,
};

#[test]
fn dummy_bootloader() {
    test_dummy_bootloader::<Vm<_, HistoryEnabled>>();
}

#[test]
fn bootloader_out_of_gas() {
    test_bootloader_out_of_gas::<Vm<_, HistoryEnabled>>();
}
