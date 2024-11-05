use crate::{
    versions::testonly::evm::{
        test_real_emulator_basics, test_real_emulator_block_info, test_real_emulator_code_hash,
        test_real_emulator_msg_info, test_real_emulator_recursion,
    },
    vm_latest::{HistoryEnabled, Vm},
};

#[test]
fn real_emulator_basics() {
    test_real_emulator_basics::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_code_hash() {
    test_real_emulator_code_hash::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_block_info() {
    test_real_emulator_block_info::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_msg_info() {
    test_real_emulator_msg_info::<Vm<_, HistoryEnabled>>();
}

#[test]
fn real_emulator_recursion() {
    test_real_emulator_recursion::<Vm<_, HistoryEnabled>>();
}
