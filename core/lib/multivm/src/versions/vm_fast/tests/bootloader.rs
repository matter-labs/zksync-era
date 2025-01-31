use crate::{
    versions::testonly::bootloader::{test_bootloader_out_of_gas, test_dummy_bootloader},
    vm_fast::Vm,
};

#[test]
fn dummy_bootloader() {
    test_dummy_bootloader::<Vm<_>>();
}

#[test]
fn bootloader_out_of_gas() {
    test_bootloader_out_of_gas::<Vm<_>>();
}
