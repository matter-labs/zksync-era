use crate::{
    versions::testonly::code_oracle::{
        test_bootloader_in_code_oracle, test_bootloader_in_create2, test_code_oracle,
        test_code_oracle_big_bytecode, test_cross_version_alias_in_code_oracle,
        test_refunds_in_code_oracle,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn code_oracle() {
    test_code_oracle::<Vm<_, HistoryEnabled>>();
}

#[test]
fn code_oracle_big_bytecode() {
    test_code_oracle_big_bytecode::<Vm<_, HistoryEnabled>>();
}

#[test]
fn refunds_in_code_oracle() {
    test_refunds_in_code_oracle::<Vm<_, HistoryEnabled>>();
}

#[test]
fn cross_version_alias_in_code_oracle() {
    test_cross_version_alias_in_code_oracle::<Vm<_, HistoryEnabled>>();
}

#[test_casing::test_casing(2, [false, true])]
fn bootloader_in_code_oracle(store_bootloader: bool) {
    test_bootloader_in_code_oracle::<Vm<_, HistoryEnabled>>(store_bootloader);
}

#[test]
fn bootloader_in_create2() {
    test_bootloader_in_create2::<Vm<_, HistoryEnabled>>();
}
